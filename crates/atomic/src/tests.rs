use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::fs::or_plain_fsync;
use crate::meta::{ReplaceFailure, recover_moved_replacement};
use crate::publish::displaced_error;
use crate::*;

struct TempDir(PathBuf);
impl TempDir {
    fn new(tag: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!("craft-atomic-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).ok();
        Self(d)
    }
    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(&self.0).map(|r| r.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
        names.sort();
        names
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const ORIGINAL: &[u8] = b"the user's precious original document";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Create,
    SyncFile,
    SyncDir,
    Rename,
    NoReplace,
    HardLink,
    Remove,
    OpenDir,
    SetPermissions,
    #[cfg(unix)]
    SetGroup,
}

#[derive(Debug, Clone, Copy)]
enum Inject {
    Kind(io::ErrorKind),
    Os(i32),
    Plain,
}

#[derive(Default)]
struct Faulty {
    faults: Vec<(Step, Inject)>,
    events: Mutex<Vec<Step>>,
    locked_renames: AtomicU32,
    replaced_during_sync: Option<PathBuf>,
    created_meanwhile: Option<PathBuf>,
    #[cfg(unix)]
    fixed_mode: Option<u32>,
}

impl Faulty {
    fn with(faults: &[(Step, Inject)]) -> Self {
        Self { faults: faults.to_vec(), ..Default::default() }
    }
    fn step(&self, step: Step) -> Option<Inject> {
        self.events.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(step);
        self.faults.iter().find(|(s, _)| *s == step).map(|(_, i)| *i)
    }
    fn count(&self, step: Step) -> usize {
        self.events.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().filter(|s| **s == step).count()
    }
}

fn error(inject: Inject) -> io::Error {
    match inject {
        Inject::Kind(kind) => io::Error::new(kind, "injected"),
        Inject::Os(code) => io::Error::from_raw_os_error(code),
        Inject::Plain => io::Error::other("plain is not an error"),
    }
}

impl AtomicFs for Faulty {
    fn create_new(&self, path: &Path, permissions: NewFilePermissions) -> io::Result<File> {
        if let Some(i) = self.step(Step::Create) {
            return Err(error(i));
        }
        let file = RealFs.create_new(path, permissions)?;
        #[cfg(unix)]
        if let Some(mode) = self.fixed_mode {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(mode))?;
        }
        Ok(file)
    }
    fn sync(&self, file: &File) -> io::Result<Flush> {
        let step = if file.metadata()?.is_dir() { Step::SyncDir } else { Step::SyncFile };
        if step == Step::SyncFile
            && let Some(target) = &self.replaced_during_sync
        {
            let theirs = target.with_extension("theirs");
            std::fs::write(&theirs, b"another program's file")?;
            std::fs::rename(&theirs, target)?;
        }
        match self.step(step) {
            Some(Inject::Plain) => RealFs.sync(file).map(|_| Flush::Plain { refused: io::Error::from_raw_os_error(45) }),
            Some(i) => Err(error(i)),
            None => RealFs.sync(file),
        }
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if let Some(i) = self.step(Step::Rename) {
            return Err(error(i));
        }
        if self.locked_renames.load(Ordering::Relaxed) > 0 {
            self.locked_renames.fetch_sub(1, Ordering::Relaxed);
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "injected: sharing violation"));
        }
        RealFs.rename(from, to)
    }
    fn rename_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
        if let Some(path) = &self.created_meanwhile {
            std::fs::write(path, ORIGINAL)?;
        }
        if let Some(i) = self.step(Step::NoReplace) {
            return Err(error(i));
        }
        RealFs.rename_noreplace(from, to)
    }
    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        if let Some(i) = self.step(Step::HardLink) {
            return Err(error(i));
        }
        RealFs.hard_link(from, to)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        if let Some(i) = self.step(Step::Remove) {
            return Err(error(i));
        }
        RealFs.remove(path)
    }
    fn open_dir(&self, dir: &Path) -> io::Result<File> {
        if let Some(i) = self.step(Step::OpenDir) {
            return Err(error(i));
        }
        RealFs.open_dir(dir)
    }
    fn set_permissions(&self, file: &File, perms: std::fs::Permissions) -> io::Result<()> {
        if let Some(i) = self.step(Step::SetPermissions) {
            return Err(error(i));
        }
        RealFs.set_permissions(file, perms)
    }
    #[cfg(unix)]
    fn set_group(&self, file: &File, gid: u32) -> io::Result<()> {
        if let Some(i) = self.step(Step::SetGroup) {
            return Err(error(i));
        }
        RealFs.set_group(file, gid)
    }
}

fn options(fs: &Faulty) -> WriteOptions<'_> {
    WriteOptions::default().with_fs(fs).with_retry(RenameRetry::NONE)
}

fn unchanged_kind(receipt: Receipt) -> Option<io::ErrorKind> {
    match receipt {
        Receipt::Unchanged(e) => Some(e.kind()),
        _ => None,
    }
}

fn reason(receipt: Receipt) -> Option<Reason> {
    match receipt {
        Receipt::VisibleUnconfirmed(u) => Some(u.reason),
        _ => None,
    }
}

fn durable(receipt: Receipt) -> bool {
    matches!(receipt, Receipt::Durable)
}

#[test]
fn a_clean_publication_is_durable_and_leaves_only_the_file() {
    let d = TempDir::new("durable");
    let p = d.0.join("doc.craft");
    let fs = Faulty::default();
    assert!(durable(options(&fs).write(&p, b"first")));
    assert!(durable(options(&fs).write(&p, b"second, longer")));
    assert_eq!(std::fs::read(&p).ok(), Some(b"second, longer".to_vec()));
    assert_eq!(d.names(), ["doc.craft"]);
}

#[test]
fn flushes_order_content_then_rename_then_folder() {
    let d = TempDir::new("order");
    let p = d.0.join("doc.craft");
    std::fs::write(&p, ORIGINAL).ok();
    let fs = Faulty::default();
    assert!(durable(options(&fs).write(&p, b"payload")));
    let events: Vec<Step> =
        fs.events.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().copied().filter(|s| *s != Step::SetPermissions).collect();
    assert_eq!(events, [Step::Create, Step::SyncFile, Step::Rename, Step::OpenDir, Step::SyncDir]);
}

#[test]
fn every_failure_before_the_replace_step_keeps_the_original_bytes() {
    let cases = [(Step::Create, io::ErrorKind::PermissionDenied), (Step::SyncFile, io::ErrorKind::StorageFull), (Step::Rename, io::ErrorKind::Other)];
    for (step, kind) in cases {
        let d = TempDir::new("before-replace");
        let p = d.0.join("doc.craft");
        std::fs::write(&p, ORIGINAL).ok();
        let fs = Faulty::with(&[(step, Inject::Kind(kind))]);
        assert_eq!(unchanged_kind(options(&fs).write(&p, &[7u8; 4096])), Some(kind), "{step:?}");
        assert_eq!(std::fs::read(&p).ok(), Some(ORIGINAL.to_vec()), "{step:?}");
        assert_eq!(d.names(), ["doc.craft"], "{step:?}");
    }
}

#[test]
fn a_failed_fill_keeps_the_original_and_removes_the_stage() {
    let d = TempDir::new("fill");
    let p = d.0.join("doc.craft");
    std::fs::write(&p, ORIGINAL).ok();
    let receipt = options(&Faulty::default()).write_with(&p, |f| {
        f.write_all(b"half a file")?;
        Err(io::Error::new(io::ErrorKind::StorageFull, "disk full"))
    });
    let Receipt::Unchanged(e) = receipt else { panic!("a failed fill is unchanged") };
    assert_eq!(e.kind(), io::ErrorKind::StorageFull);
    assert!(e.to_string().ends_with("the original file was not changed"), "{e}");
    assert_eq!(std::fs::read(&p).ok(), Some(ORIGINAL.to_vec()));
    assert_eq!(d.names(), ["doc.craft"]);
}

#[test]
fn a_destination_replaced_while_staging_is_left_to_its_new_owner() {
    let d = TempDir::new("identity");
    let p = d.0.join("doc.craft");
    std::fs::write(&p, ORIGINAL).ok();
    let fs = Faulty { replaced_during_sync: Some(p.clone()), ..Default::default() };
    let Receipt::Unchanged(e) = options(&fs).write(&p, b"mine") else { panic!("a swapped destination is refused") };
    assert!(e.to_string().contains("another program replaced the file"), "{e}");
    assert_eq!(std::fs::read(&p).ok(), Some(b"another program's file".to_vec()));
    assert_eq!(fs.count(Step::Rename), 0);
    assert_eq!(d.names(), ["doc.craft"]);
}

#[test]
fn a_destination_created_while_staging_is_not_replaced() {
    let d = TempDir::new("identity-new");
    let p = d.0.join("doc.craft");
    let fs = Faulty { replaced_during_sync: Some(p.clone()), ..Default::default() };
    assert_eq!(unchanged_kind(options(&fs).write(&p, b"mine")), Some(io::ErrorKind::Other));
    assert_eq!(std::fs::read(&p).ok(), Some(b"another program's file".to_vec()));
}

#[test]
fn each_unconfirmed_reason_is_reported_after_the_file_is_in_place() {
    let cases = [
        (vec![(Step::SyncFile, Inject::Plain)], Reason::FullFlushRefused),
        (vec![(Step::SyncDir, Inject::Plain)], Reason::FullFlushRefused),
        (vec![(Step::OpenDir, Inject::Kind(io::ErrorKind::PermissionDenied))], Reason::DirOpenDenied),
        (vec![(Step::OpenDir, Inject::Kind(io::ErrorKind::Unsupported))], Reason::DirSyncUnsupported),
        (vec![(Step::OpenDir, Inject::Kind(io::ErrorKind::NotFound))], Reason::DirSyncFailed),
        (vec![(Step::SyncDir, Inject::Os(22))], Reason::DirSyncUnsupported),
        (vec![(Step::SyncDir, Inject::Os(5))], Reason::DirSyncFailed),
        (vec![(Step::SyncFile, Inject::Plain), (Step::SyncDir, Inject::Os(5))], Reason::DirSyncFailed),
    ];
    for (faults, expected) in cases {
        let d = TempDir::new("unconfirmed");
        let p = d.0.join("doc.craft");
        std::fs::write(&p, ORIGINAL).ok();
        let fs = Faulty::with(&faults);
        assert_eq!(reason(options(&fs).write(&p, b"new bytes")), Some(expected), "{faults:?}");
        assert_eq!(std::fs::read(&p).ok(), Some(b"new bytes".to_vec()), "{faults:?}");
        assert_eq!(d.names(), ["doc.craft"], "{faults:?}");
    }
}

#[test]
fn unconfirmed_receipts_still_count_as_visible() {
    let d = TempDir::new("visible");
    let p = d.0.join("doc.craft");
    let fs = Faulty::with(&[(Step::SyncDir, Inject::Os(5))]);
    let receipt = options(&fs).write(&p, b"x");
    let Receipt::VisibleUnconfirmed(u) = receipt else { panic!("a failed folder flush is unconfirmed") };
    assert_eq!(u.reason.id(), "dir-sync-failed");
    assert!(u.to_string().starts_with("the file is in place, but flushing its folder failed"), "{u}");
    let refused =
        Receipt::VisibleUnconfirmed(Unconfirmed { reason: u.reason, error: io::Error::from_raw_os_error(5) }).durable().err().map(|e| e.to_string());
    assert!(refused.as_deref().is_some_and(|m| m.starts_with("the file is in place, but flushing its folder failed")), "{refused:?}");
    assert!(Receipt::VisibleUnconfirmed(u).visible().is_ok());
    assert!(Receipt::Durable.durable().is_ok());
    assert!(Receipt::Durable.visible().is_ok());
    assert_eq!(Receipt::Unchanged(io::ErrorKind::StorageFull.into()).visible().map_err(|e| e.kind()), Err(io::ErrorKind::StorageFull));
}

#[test]
fn locked_renames_are_retried_with_capped_delays() {
    let d = TempDir::new("locked");
    let p = d.0.join("doc.craft");
    std::fs::write(&p, ORIGINAL).ok();
    let fs = Faulty { locked_renames: AtomicU32::new(2), ..Default::default() };
    let started = std::time::Instant::now();
    let receipt = options(&fs).with_retry(RenameRetry { attempts: 2, first_delay_ms: 60_000 }).write(&p, b"new");
    assert!(durable(receipt));
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(fs.count(Step::Rename), 3);
    let fs = Faulty { locked_renames: AtomicU32::new(2), ..Default::default() };
    assert_eq!(
        unchanged_kind(options(&fs).with_retry(RenameRetry { attempts: 1, first_delay_ms: 0 }).write(&p, b"newer")),
        Some(io::ErrorKind::PermissionDenied)
    );
    assert_eq!(std::fs::read(&p).ok(), Some(b"new".to_vec()));
}

#[test]
fn a_dropped_stage_removes_only_its_own_file() {
    let d = TempDir::new("stage");
    let p = d.0.join("movie.mp4");
    std::fs::write(&p, ORIGINAL).ok();
    let fs = Faulty::default();
    let mut stage = options(&fs).stage(&p).ok().unwrap_or_else(|| panic!("stage"));
    stage.file().write_all(b"partial frames").ok();
    assert_eq!(d.names().len(), 2);
    drop(stage);
    assert_eq!(std::fs::read(&p).ok(), Some(ORIGINAL.to_vec()));
    assert_eq!(d.names(), ["movie.mp4"]);
    let mut stage = options(&fs).stage(&p).ok().unwrap_or_else(|| panic!("stage"));
    stage.file().write_all(b"whole movie").ok();
    assert!(durable(stage.publish()));
    assert_eq!(std::fs::read(&p).ok(), Some(b"whole movie".to_vec()));
    assert_eq!(d.names(), ["movie.mp4"]);
}

#[test]
fn a_missing_folder_is_an_error_naming_it_and_is_never_created() {
    let d = TempDir::new("parent");
    let folder = d.0.join("a");
    let p = folder.join("b/out.png");
    for receipt in [write(&p, b"x"), write_new(&p, b"x")] {
        let Receipt::Unchanged(e) = receipt else { panic!("a missing folder is unchanged") };
        assert!(e.to_string().contains(&folder.join("b").display().to_string()), "{e}");
    }
    assert!(!folder.exists());
}

#[test]
fn write_new_never_replaces_and_publishes_with_an_exclusive_rename() {
    let d = TempDir::new("new");
    let p = d.0.join("actions.atn");
    let fs = Faulty::default();
    assert!(durable(options(&fs).write_new(&p, b"first")));
    assert_eq!(fs.count(Step::NoReplace), 1);
    assert_eq!(fs.count(Step::HardLink), 0);
    assert_eq!(unchanged_kind(options(&fs).write_new(&p, b"second")), Some(io::ErrorKind::AlreadyExists));
    assert_eq!(std::fs::read(&p).ok(), Some(b"first".to_vec()));
    assert_eq!(d.names(), ["actions.atn"]);
}

#[test]
fn write_new_keeps_a_file_that_appears_meanwhile_on_every_publication_route() {
    let routes: [&[(Step, Inject)]; 3] = [
        &[],
        &[(Step::NoReplace, Inject::Kind(io::ErrorKind::Unsupported))],
        &[(Step::NoReplace, Inject::Kind(io::ErrorKind::Unsupported)), (Step::HardLink, Inject::Kind(io::ErrorKind::PermissionDenied))],
    ];
    for faults in routes {
        let d = TempDir::new("new-race");
        let p = d.0.join("export.png");
        let fs = Faulty { created_meanwhile: Some(p.clone()), ..Faulty::with(faults) };
        assert_eq!(unchanged_kind(options(&fs).write_new(&p, b"mine")), Some(io::ErrorKind::AlreadyExists), "{faults:?}");
        assert_eq!(std::fs::read(&p).ok(), Some(ORIGINAL.to_vec()), "{faults:?}");
        assert_eq!(d.names(), ["export.png"], "{faults:?}");
    }
}

#[test]
fn write_new_falls_back_to_a_link_then_a_copy() {
    let unsupported = (Step::NoReplace, Inject::Kind(io::ErrorKind::Unsupported));
    let d = TempDir::new("new-fallback");
    let linked = d.0.join("linked.png");
    let fs = Faulty::with(&[unsupported]);
    assert!(durable(options(&fs).write_new(&linked, b"linked")));
    assert_eq!(fs.count(Step::HardLink), 1);
    let copied = d.0.join("copied.png");
    let fs = Faulty::with(&[unsupported, (Step::HardLink, Inject::Kind(io::ErrorKind::PermissionDenied))]);
    assert!(durable(options(&fs).write_new(&copied, b"copied")));
    assert_eq!(fs.count(Step::SyncFile), 2);
    assert_eq!(std::fs::read(&linked).ok(), Some(b"linked".to_vec()));
    assert_eq!(std::fs::read(&copied).ok(), Some(b"copied".to_vec()));
    assert_eq!(d.names(), ["copied.png", "linked.png"]);
}

#[test]
fn a_new_file_that_could_not_be_staged_mentions_no_original() {
    let d = TempDir::new("new-full");
    let p = d.0.join("full.png");
    let fs = Faulty::with(&[(Step::SyncFile, Inject::Kind(io::ErrorKind::StorageFull))]);
    let Receipt::Unchanged(e) = options(&fs).write_new(&p, b"x") else { panic!("a failed flush is unchanged") };
    let msg = e.to_string();
    assert!(msg.contains("disk is full") && !msg.contains("original"), "{msg}");
    assert!(d.names().is_empty());
}

#[test]
fn ordinary_exclusive_rename_errors_never_try_another_route() {
    let d = TempDir::new("new-ordinary");
    let p = d.0.join("out.png");
    let fs = Faulty::with(&[(Step::NoReplace, Inject::Kind(io::ErrorKind::PermissionDenied))]);
    assert_eq!(unchanged_kind(options(&fs).write_new(&p, b"x")), Some(io::ErrorKind::PermissionDenied));
    assert_eq!(fs.count(Step::HardLink), 0);
    assert!(d.names().is_empty());
}

#[cfg(unix)]
#[test]
fn links_are_followed_by_default_and_replaced_on_request() {
    let d = TempDir::new("links");
    let target = d.0.join("real.craft");
    let link = d.0.join("link.craft");
    std::fs::write(&target, ORIGINAL).ok();
    std::os::unix::fs::symlink(&target, &link).ok();
    assert!(write(&link, b"through").visible().is_ok());
    assert_eq!(std::fs::read(&target).ok(), Some(b"through".to_vec()));
    assert!(std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()));
    assert!(WriteOptions::default().with_links(Links::Replace).write(&link, b"replaced").visible().is_ok());
    assert_eq!(std::fs::read(&target).ok(), Some(b"through".to_vec()));
    assert!(std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_file()));
    assert_eq!(std::fs::read(&link).ok(), Some(b"replaced".to_vec()));
}

#[cfg(unix)]
#[test]
fn a_read_only_file_is_refused_before_anything_is_written() {
    use std::os::unix::fs::PermissionsExt;
    let d = TempDir::new("read-only");
    let p = d.0.join("doc.craft");
    std::fs::write(&p, ORIGINAL).ok();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o444)).ok();
    let fs = Faulty::default();
    let Receipt::Unchanged(e) = options(&fs).write(&p, b"new") else { panic!("read-only is refused") };
    assert!(e.to_string().contains("read-only"), "{e}");
    assert!(fs.events.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_empty());
    assert_eq!(std::fs::read(&p).ok(), Some(ORIGINAL.to_vec()));
}

#[cfg(unix)]
#[test]
fn replacing_keeps_the_mode_and_a_failed_mode_change_fails_the_save() {
    use std::os::unix::fs::PermissionsExt;
    let d = TempDir::new("mode");
    let p = d.0.join("doc.craft");
    std::fs::write(&p, ORIGINAL).ok();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o640)).ok();
    assert!(durable(options(&Faulty::default()).write(&p, b"kept mode")));
    assert_eq!(std::fs::metadata(&p).map(|m| m.permissions().mode() & 0o7777).ok(), Some(0o640));
    let fs = Faulty::with(&[(Step::SetPermissions, Inject::Kind(io::ErrorKind::Unsupported))]);
    let Receipt::Unchanged(e) = options(&fs).write(&p, b"lost mode") else { panic!("a required mode change must succeed") };
    assert!(e.to_string().contains("permissions"), "{e}");
    assert_eq!(std::fs::read(&p).ok(), Some(b"kept mode".to_vec()));
    assert_eq!(fs.count(Step::Rename), 0);
    assert_eq!(d.names(), ["doc.craft"]);
}

#[cfg(unix)]
#[test]
fn matching_fixed_modes_need_no_chmod() {
    use std::os::unix::fs::PermissionsExt;
    let d = TempDir::new("fixed-mode");
    let p = d.0.join("doc.craft");
    std::fs::write(&p, ORIGINAL).ok();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).ok();
    let fs = Faulty { fixed_mode: Some(0o700), ..Faulty::with(&[(Step::SetPermissions, Inject::Kind(io::ErrorKind::Unsupported))]) };
    assert!(durable(options(&fs).write(&p, b"replacement")));
    assert_eq!(fs.count(Step::SetPermissions), 0);
}

#[cfg(unix)]
#[test]
fn a_callback_mode_change_does_not_survive_a_replacement() {
    use std::os::unix::fs::PermissionsExt;
    let d = TempDir::new("callback-mode");
    let p = d.0.join("doc.craft");
    std::fs::write(&p, ORIGINAL).ok();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).ok();
    let receipt = options(&Faulty::default()).write_with(&p, |f| {
        f.write_all(b"replacement")?;
        f.set_permissions(std::fs::Permissions::from_mode(0o4644))
    });
    assert!(durable(receipt));
    assert_eq!(std::fs::metadata(&p).map(|m| m.permissions().mode() & 0o7777).ok(), Some(0o600));
}

#[cfg(unix)]
#[test]
fn group_bits_are_dropped_when_the_stage_cannot_join_the_replaced_group() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let d = TempDir::new("group");
    let Ok(file) = File::create(d.0.join("stage")) else { panic!("stage") };
    let gid = file.metadata().map(|m| m.gid()).unwrap_or_default();
    let elsewhere = gid.wrapping_add(1);
    let fs = Faulty::with(&[(Step::SetGroup, Inject::Kind(io::ErrorKind::PermissionDenied))]);
    let adopt =
        |stage, replaced, mode| crate::publish::adopt_group(&fs, &file, stage, replaced, std::fs::Permissions::from_mode(mode)).mode() & 0o777;
    assert_eq!(adopt(gid, elsewhere, 0o640), 0o600);
    assert_eq!(adopt(gid, elsewhere, 0o600), 0o600);
    assert_eq!(adopt(gid, gid, 0o640), 0o640);
    assert_eq!(fs.count(Step::SetGroup), 1);
}

#[cfg(any(target_os = "linux", target_vendor = "apple"))]
#[test]
fn extended_attributes_are_carried_unless_only_the_mode_is_asked_for() {
    let d = TempDir::new("xattr");
    let name: &[u8] = if cfg!(target_vendor = "apple") { b"com.craft.tag" } else { b"user.craft.tag" };
    let read = |p: &Path| -> Option<Vec<u8>> {
        let f = File::open(p).ok()?;
        let mut buf = vec![0u8; 64];
        let n = rustix::fs::fgetxattr(&f, name, &mut buf).ok()?;
        buf.truncate(n);
        Some(buf)
    };
    for (metadata, expected) in [(Metadata::Carry, Some(b"red".to_vec())), (Metadata::ModeOnly, None)] {
        let p = d.0.join(format!("{metadata:?}.craft"));
        std::fs::write(&p, ORIGINAL).ok();
        let Ok(f) = File::open(&p) else { panic!("open") };
        if rustix::fs::fsetxattr(&f, name, b"red", rustix::fs::XattrFlags::empty()).is_err() {
            return;
        }
        drop(f);
        assert!(WriteOptions::default().with_metadata(metadata).write(&p, b"tagged").visible().is_ok());
        assert_eq!(read(&p), expected, "{metadata:?}");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn the_creation_time_is_carried_across_a_replacement() {
    let d = TempDir::new("created");
    let p = d.0.join("doc.craft");
    std::fs::write(&p, ORIGINAL).ok();
    let before = std::fs::metadata(&p).and_then(|m| m.created()).ok();
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert!(write(&p, b"later").visible().is_ok());
    assert_eq!(std::fs::metadata(&p).and_then(|m| m.created()).ok(), before);
}

#[test]
fn temp_names_are_visible_bounded_and_owned() {
    let name = temp_name("document.craft");
    assert!(name.starts_with("document.craft.") && name.ends_with(".craft_tmp"), "{name}");
    assert_eq!(temp_owner(&name), Some(std::process::id()));
    let long = "é".repeat(100);
    let bounded = temp_name(&long);
    assert!(bounded.len() <= 64 + ".4294967295-18446744073709551615.craft_tmp".len(), "{}", bounded.len());
    assert!(bounded.starts_with(&"é".repeat(32)) && !bounded.starts_with(&"é".repeat(33)));
    assert_eq!(temp_owner(&bounded), Some(std::process::id()));
    assert_eq!(temp_owner("doc.psd.42-7.pcsave.tmp"), Some(42));
    assert_eq!(temp_owner(".doc.psd.42-7.pcsave.tmp"), Some(42));
    assert_eq!(temp_owner(".doc.psd.42-7.craft_tmp"), Some(42));
    for other in ["doc.psd", "doc.psd.042-7.craft_tmp", "doc.psd.42-.craft_tmp", ".42-7.craft_tmp", "doc.psd.x-7.craft_tmp", "doc.psd.42-7.tmp"] {
        assert_eq!(temp_owner(other), None, "{other}");
    }
}

#[test]
fn a_refused_full_flush_falls_back_and_says_so() {
    let refused = || Err(io::Error::from_raw_os_error(45));
    let plain = or_plain_fsync(refused(), || Ok(()));
    if cfg!(target_vendor = "apple") {
        assert!(matches!(plain, Ok(Flush::Plain { .. })));
        assert_eq!(
            or_plain_fsync(refused(), || Err(io::ErrorKind::StorageFull.into())).map_err(|e| e.kind()).err(),
            Some(io::ErrorKind::StorageFull)
        );
    } else {
        assert_eq!(plain.err().and_then(|e| e.raw_os_error()), Some(45));
    }
    assert!(matches!(or_plain_fsync(Ok(()), || Err(io::ErrorKind::Other.into())), Ok(Flush::Full)));
}

#[test]
fn a_moved_replacement_completes_restores_or_names_where_the_original_is() {
    let original = PathBuf::from("doc.psd.1-0.craft_tmp");
    assert!(recover_moved_replacement(|| Ok(()), || panic!("no restore after completion"), original.clone()).is_ok());
    let restored = recover_moved_replacement(|| Err(io::ErrorKind::PermissionDenied.into()), || Ok(()), original.clone());
    assert!(matches!(restored, Err(ReplaceFailure::Kept(e)) if e.kind() == io::ErrorKind::PermissionDenied));
    let lost = recover_moved_replacement(|| Err(io::ErrorKind::Other.into()), || Err(io::ErrorKind::Other.into()), original.clone());
    let Err(ReplaceFailure::Displaced { completion, restoration, original: at }) = lost else { panic!("both failed") };
    assert_eq!(at, original);
    let msg = displaced_error(Path::new("doc.psd"), &completion, &restoration, &at).to_string();
    assert!(msg.contains("your document survives at doc.psd.1-0.craft_tmp") && !msg.contains("not changed"), "{msg}");
    let gone = displaced_error(Path::new("doc.psd"), &completion, &io::ErrorKind::NotFound.into(), &at).to_string();
    assert!(gone.contains("no longer exists") && !gone.contains("survives"), "{gone}");
}

#[test]
fn identity_tells_a_replaced_file_from_the_same_one() {
    let d = TempDir::new("identity-type");
    let p = d.0.join("frame.png");
    std::fs::write(&p, b"one").ok();
    let first = Identity::at(&p).ok();
    assert_eq!(Identity::at(&p).ok(), first);
    assert!(write(&p, b"two, longer").visible().is_ok());
    assert!(first.is_some());
    assert_ne!(Identity::at(&p).ok(), first);
}

#[test]
fn a_handle_relative_writer_gets_the_same_receipt() {
    let d = TempDir::new("confirm");
    assert!(durable(confirm(&RealFs, &d.0, Flush::Full, RealFs.open_dir(&d.0))));
    assert_eq!(
        reason(confirm(&RealFs, &d.0, Flush::Plain { refused: io::Error::from_raw_os_error(45) }, RealFs.open_dir(&d.0))),
        Some(Reason::FullFlushRefused)
    );
    assert_eq!(reason(confirm(&RealFs, &d.0, Flush::Full, Err(io::ErrorKind::PermissionDenied.into()))), Some(Reason::DirOpenDenied));
    assert_eq!(reason(confirm(&RealFs, &d.0, Flush::Full, Err(io::ErrorKind::Unsupported.into()))), Some(Reason::DirSyncUnsupported));
    assert_eq!(reason(confirm(&RealFs, &d.0, Flush::Full, Err(io::Error::from_raw_os_error(5)))), Some(Reason::DirSyncFailed));
}

#[test]
fn a_stage_is_a_seekable_writer() {
    use std::io::{Seek, SeekFrom};
    let d = TempDir::new("stage-writer");
    let p = d.0.join("movie.mp4");
    let fs = Faulty::default();
    let Ok(mut stage) = options(&fs).stage(&p) else { panic!("stage") };
    assert!(stage.write_all(b"....body").is_ok());
    assert!(stage.seek(SeekFrom::Start(0)).is_ok());
    assert!(stage.write_all(b"head").is_ok());
    assert!(durable(stage.publish()));
    assert_eq!(std::fs::read(&p).ok(), Some(b"headbody".to_vec()));
}
