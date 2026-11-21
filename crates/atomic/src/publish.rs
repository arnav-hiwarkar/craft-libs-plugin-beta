use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use crate::fs::{dir_sync_unsupported, is_transient_lock, rename_noreplace_unsupported, retry_when};
use crate::meta::ReplaceFailure;
use crate::{AtomicFs, Entry, EntryKind, Flush, Links, Metadata, NewFilePermissions, Reason, Receipt, Unconfirmed, WriteOptions, temp_name};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
    #[cfg(not(unix))]
    len: u64,
    #[cfg(not(unix))]
    modified: Option<std::time::SystemTime>,
    #[cfg(not(unix))]
    created: Option<std::time::SystemTime>,
}

impl Identity {
    #[cfg(unix)]
    pub const fn new(dev: u64, ino: u64) -> Self {
        Self { dev, ino }
    }

    #[cfg(not(unix))]
    pub const fn new(len: u64, modified: Option<std::time::SystemTime>, created: Option<std::time::SystemTime>) -> Self {
        Self { len, modified, created }
    }
}

pub struct Stage<'fs> {
    options: WriteOptions<'fs>,
    path: PathBuf,
    target: PathBuf,
    dir: PathBuf,
    file: File,
    tmp: TempGuard<'fs>,
    kind: Kind,
}

enum Kind {
    Replace(Option<Entry>),
    New,
}

struct TempGuard<'fs> {
    fs: &'fs dyn AtomicFs,
    path: PathBuf,
    armed: bool,
}

impl Drop for TempGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.fs.remove(&self.path);
        }
    }
}

impl TempGuard<'_> {
    fn remove(mut self) -> io::Result<()> {
        self.armed = false;
        match self.fs.remove(&self.path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

impl<'fs> Stage<'fs> {
    pub(crate) fn replace(options: WriteOptions<'fs>, path: &Path) -> io::Result<Self> {
        let fs = options.fs;
        let target = resolve(fs, options.links, path)?;
        let (dir, name) = split(&target, path)?;
        let replaced = fs.entry(&target).ok();
        if let Some(entry) = replaced.filter(|e| e.kind == EntryKind::File) {
            check_writable(fs, &target, &entry).map_err(|e| read_only_error(e, path))?;
        }
        let creation = if replaced.is_some() { NewFilePermissions::Private } else { options.new_file_permissions };
        let (tmp, file) = create_stage(fs, &dir, &name, creation).map_err(|e| write_error(e, path, &dir, replaced.is_some()))?;
        Ok(Self { options, path: path.to_path_buf(), target, dir, file, tmp, kind: Kind::Replace(replaced) })
    }

    pub(crate) fn create_new(options: WriteOptions<'fs>, path: &Path) -> io::Result<Self> {
        let fs = options.fs;
        let target = resolve(fs, options.links, path)?;
        let (dir, name) = split(&target, path)?;
        if fs.entry(&target).is_ok() {
            return Err(write_error(io::ErrorKind::AlreadyExists.into(), path, &dir, true));
        }
        let (tmp, file) = create_stage(fs, &dir, &name, options.new_file_permissions).map_err(|e| write_error(e, path, &dir, false))?;
        Ok(Self { options, path: path.to_path_buf(), target, dir, file, tmp, kind: Kind::New })
    }

    pub fn replaces(&self) -> bool {
        matches!(self.kind, Kind::Replace(Some(_)))
    }

    pub(crate) fn fill_and_publish(mut self, fill: impl FnOnce(&mut File) -> io::Result<()>) -> Receipt {
        match fill(&mut self.file) {
            Ok(()) => self.publish(),
            Err(e) => Receipt::Unchanged(write_error(e, &self.path, &self.dir, self.replaces())),
        }
    }

    pub fn publish(self) -> Receipt {
        let existed = self.replaces();
        let Stage { options, path, target, dir, file, mut tmp, kind } = self;
        let fs = options.fs;
        let seal = |file: File| -> Result<Flush, Receipt> {
            if let Kind::Replace(Some(entry)) = &kind
                && entry.kind == EntryKind::File
            {
                if options.metadata == Metadata::Carry {
                    crate::meta::carry(fs, &target, &file, &tmp.path);
                }
                keep_mode(fs, &file, entry).map_err(|e| Receipt::Unchanged(permissions_error(e, &path)))?;
            }
            fs.sync(&file).map_err(|e| Receipt::Unchanged(write_error(e, &path, &dir, existed)))
        };
        let mut causes = Causes::default();
        match seal(file) {
            Ok(flush) => causes.flush(flush),
            Err(receipt) => return receipt,
        }
        let placed = match kind {
            Kind::Replace(Some(entry)) => {
                if fs.entry(&target).ok().map(|now| now.identity) != Some(entry.identity) {
                    return Receipt::Unchanged(changed_error(&path));
                }
                let carry = entry.kind == EntryKind::File && options.metadata == Metadata::Carry;
                match swap(options, &tmp.path, &target, carry) {
                    Ok(()) => Placed::Renamed,
                    Err(ReplaceFailure::Kept(e)) => return Receipt::Unchanged(rename_error(e, &path, existed)),
                    Err(ReplaceFailure::Displaced { completion, restoration, original }) => {
                        let error = displaced_error(&path, &completion, &restoration, &original);
                        log::error!("{error}");
                        return Receipt::Displaced { error, original };
                    }
                }
            }
            Kind::Replace(None) | Kind::New => match place_new(options, &tmp.path, &target) {
                Ok(placed) => placed,
                Err(e) => {
                    let taken = e.kind() == io::ErrorKind::AlreadyExists;
                    return Receipt::Unchanged(write_error(e, &path, &dir, taken));
                }
            },
        };
        match placed {
            Placed::Renamed => tmp.armed = false,
            Placed::Linked => causes.stage_removal(tmp.remove()),
            Placed::Copied(flush) => {
                causes.flush(flush);
                causes.stage_removal(tmp.remove());
            }
        }
        causes.folder(fs, &dir);
        causes.receipt(&path)
    }
}

impl io::Write for Stage<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl io::Seek for Stage<'_> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.file.seek(pos)
    }
}

#[derive(Default)]
struct Causes(Vec<(Reason, io::Error)>);

impl Causes {
    fn flush(&mut self, flush: Flush) {
        if let Flush::Plain { refused } = flush
            && !self.0.iter().any(|(r, _)| *r == Reason::FullFlushRefused)
        {
            self.0.push((Reason::FullFlushRefused, refused));
        }
    }

    fn stage_removal(&mut self, removed: io::Result<()>) {
        if let Err(e) = removed {
            self.0.push((Reason::StageRemovalFailed, e));
        }
    }

    fn folder(&mut self, fs: &dyn AtomicFs, dir: &Path) {
        match fs.open_dir(dir) {
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => self.0.push((Reason::DirOpenDenied, e)),
            Err(e) if dir_sync_unsupported(&e) => self.0.push((Reason::DirSyncUnsupported, e)),
            Err(e) => self.0.push((Reason::DirSyncFailed, e)),
            Ok(handle) => match fs.sync(&handle) {
                Ok(flush) => self.flush(flush),
                Err(e) if dir_sync_unsupported(&e) => self.0.push((Reason::DirSyncUnsupported, e)),
                Err(e) => self.0.push((Reason::DirSyncFailed, e)),
            },
        }
    }

    fn receipt(self, path: &Path) -> Receipt {
        let mut causes = self.0.into_iter();
        let Some(first) = causes.next() else { return Receipt::Durable };
        let unconfirmed = Unconfirmed::new(first, causes.collect());
        log::warn!("{}: {unconfirmed}", path.display());
        Receipt::VisibleUnconfirmed(unconfirmed)
    }
}

fn swap(options: WriteOptions<'_>, from: &Path, to: &Path, carry: bool) -> Result<(), ReplaceFailure> {
    let fs = options.fs;
    if carry {
        let transient = |e: &ReplaceFailure| matches!(e, ReplaceFailure::Kept(e) if is_transient_lock(e));
        return retry_when(options.retry, transient, || fs.replace_existing(from, to));
    }
    retry_when(options.retry, is_transient_lock, || fs.rename(from, to)).map_err(ReplaceFailure::Kept)
}

enum Placed {
    Renamed,
    Linked,
    Copied(Flush),
}

fn place_new(options: WriteOptions<'_>, tmp: &Path, target: &Path) -> io::Result<Placed> {
    let fs = options.fs;
    match fs.rename_noreplace(tmp, target) {
        Ok(()) => return Ok(Placed::Renamed),
        Err(e) if !rename_noreplace_unsupported(&e) => return Err(e),
        Err(_) => {}
    }
    match fs.hard_link(tmp, target) {
        Ok(()) => Ok(Placed::Linked),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(e),
        Err(_) => copy_new(fs, tmp, target, options.new_file_permissions).map(Placed::Copied),
    }
}

fn copy_new(fs: &dyn AtomicFs, tmp: &Path, target: &Path, permissions: NewFilePermissions) -> io::Result<Flush> {
    let mut to = fs.create_new(target, permissions)?;
    let copied = fs.open_read(tmp).and_then(move |mut from| {
        #[cfg(unix)]
        let final_permissions = {
            use std::os::unix::fs::PermissionsExt;
            let source = from.metadata()?.permissions();
            let current = to.metadata()?.permissions();
            (source.mode() != current.mode()).then_some(source)
        };
        #[cfg(unix)]
        if permissions == NewFilePermissions::PlatformDefault
            && let Some(ref mode) = final_permissions
        {
            fs.set_permissions(&to, mode.clone())?;
        }
        io::copy(&mut from, &mut to)?;
        #[cfg(unix)]
        if permissions == NewFilePermissions::Private
            && let Some(mode) = final_permissions
        {
            fs.set_permissions(&to, mode)?;
        }
        fs.sync(&to)
    });
    if copied.is_err() {
        let _ = fs.remove(target);
    }
    copied
}

const STAGE_ATTEMPTS: usize = 8;

fn create_stage<'fs>(fs: &'fs dyn AtomicFs, dir: &Path, name: &str, permissions: NewFilePermissions) -> io::Result<(TempGuard<'fs>, File)> {
    for _ in 0..STAGE_ATTEMPTS {
        let path = dir.join(temp_name(name));
        match fs.create_new(&path, permissions) {
            Ok(file) => return Ok((TempGuard { fs, path, armed: true }, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other(format!("{}: no unused temporary file name", dir.display())))
}

#[cfg(unix)]
fn keep_mode(fs: &dyn AtomicFs, file: &File, replaced: &Entry) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let staged = file.metadata()?;
    if staged.permissions().mode() & 0o7777 == replaced.mode & 0o7777 {
        return Ok(());
    }
    let perms = adopt_group(fs, file, staged.gid(), replaced.gid, std::fs::Permissions::from_mode(replaced.mode & 0o7777));
    fs.set_permissions(file, perms)
}

#[cfg(not(unix))]
fn keep_mode(_fs: &dyn AtomicFs, _file: &File, _replaced: &Entry) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
pub(crate) fn adopt_group(fs: &dyn AtomicFs, file: &File, stage_gid: u32, replaced_gid: u32, perms: std::fs::Permissions) -> std::fs::Permissions {
    use std::os::unix::fs::PermissionsExt;
    if perms.mode() & 0o070 == 0 || stage_gid == replaced_gid || fs.set_group(file, replaced_gid).is_ok() {
        return perms;
    }
    let mut narrowed = perms;
    narrowed.set_mode(narrowed.mode() & !0o070);
    narrowed
}

#[cfg(unix)]
fn check_writable(fs: &dyn AtomicFs, target: &Path, _entry: &Entry) -> io::Result<()> {
    fs.open_write(target).map(drop)
}

#[cfg(not(unix))]
fn check_writable(_fs: &dyn AtomicFs, _target: &Path, entry: &Entry) -> io::Result<()> {
    if entry.readonly {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the file is marked read-only"));
    }
    Ok(())
}

fn resolve(fs: &dyn AtomicFs, links: Links, path: &Path) -> io::Result<PathBuf> {
    match links {
        Links::Follow => resolve_links(fs, path),
        Links::Replace => Ok(path.to_path_buf()),
    }
}

const MAX_LINKS: usize = 40;

fn resolve_links(fs: &dyn AtomicFs, path: &Path) -> io::Result<PathBuf> {
    let mut p = path.to_path_buf();
    for _ in 0..MAX_LINKS {
        match fs.entry(&p) {
            Ok(e) if e.kind == EntryKind::Link => {
                let to = fs.read_link(&p).map_err(|e| io::Error::new(e.kind(), format!("cannot save {}: {e}", path.display())))?;
                p = match p.parent() {
                    Some(dir) if to.is_relative() => dir.join(to),
                    _ => to,
                };
            }
            _ => return Ok(p),
        }
    }
    Err(io::Error::new(io::ErrorKind::InvalidInput, format!("cannot save {}: too many levels of symbolic links", path.display())))
}

fn split(target: &Path, path: &Path) -> io::Result<(PathBuf, String)> {
    let name = target.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{}: not a file path", path.display())))?;
    let dir = match target.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    Ok((dir, name.to_string_lossy().into_owned()))
}

fn unchanged(existed: bool) -> &'static str {
    if existed { "; the original file was not changed" } else { "" }
}

fn write_error(e: io::Error, path: &Path, dir: &Path, existed: bool) -> io::Error {
    let kept = unchanged(existed);
    let msg = match e.kind() {
        io::ErrorKind::AlreadyExists => format!("cannot save {}: the file already exists{kept}", path.display()),
        io::ErrorKind::NotFound => format!("cannot save {}: the folder {} does not exist ({e}){kept}", path.display(), dir.display()),
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
            format!("cannot save {}: the folder {} is not writable ({e}){kept}", path.display(), dir.display())
        }
        io::ErrorKind::StorageFull => format!("cannot save {}: the disk is full ({e}){kept}", path.display()),
        _ => format!("cannot save {}: {e}{kept}", path.display()),
    };
    io::Error::new(e.kind(), msg)
}

fn permissions_error(e: io::Error, path: &Path) -> io::Error {
    io::Error::new(
        e.kind(),
        format!("cannot save {}: the new file cannot be given the original's permissions ({e}){}", path.display(), unchanged(true)),
    )
}

fn read_only_error(e: io::Error, path: &Path) -> io::Error {
    let kept = unchanged(true);
    let msg = match e.kind() {
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
            format!("cannot save {}: the file is read-only ({e}){kept}", path.display())
        }
        _ => format!("cannot save {}: {e}{kept}", path.display()),
    };
    io::Error::new(e.kind(), msg)
}

fn changed_error(path: &Path) -> io::Error {
    io::Error::other(format!("cannot save {}: another program replaced the file while it was being saved; that file was not changed", path.display()))
}

fn rename_error(e: io::Error, path: &Path, existed: bool) -> io::Error {
    let kept = unchanged(existed);
    let msg = if is_cross_device(&e) {
        format!("cannot save {}: the temporary file is on a different volume than the destination ({e}){kept}", path.display())
    } else if e.kind() == io::ErrorKind::PermissionDenied {
        format!("cannot save {}: the file is locked or read-only ({e}){kept}", path.display())
    } else {
        format!("cannot save {}: {e}{kept}", path.display())
    };
    io::Error::new(e.kind(), msg)
}

pub(crate) fn displaced_error(path: &Path, completion: &io::Error, restoration: &io::Error, original: &Path) -> io::Error {
    let msg = if restoration.kind() == io::ErrorKind::NotFound {
        format!(
            "cannot save {}: the previous version moved to its backup name but could not be restored from it (completing the save failed: {completion}; the backup {} no longer exists: {restoration}), so no file is currently at that name",
            path.display(),
            original.display()
        )
    } else {
        format!(
            "cannot save {}: the previous version moved to its backup name but could not be restored from it (completing the save failed: {completion}; restoring the backup failed: {restoration}); your document survives at {}, recover it from there before saving again",
            path.display(),
            original.display()
        )
    };
    io::Error::other(msg)
}

fn is_cross_device(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::CrossesDevices || (cfg!(unix) && e.raw_os_error() == Some(18)) || (cfg!(windows) && e.raw_os_error() == Some(17))
}
