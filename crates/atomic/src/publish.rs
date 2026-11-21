use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use crate::fs::{dir_sync_unsupported, is_transient_lock, rename_noreplace_unsupported, retry_when};
use crate::meta::ReplaceFailure;
use crate::{AtomicFs, Flush, Links, Metadata, NewFilePermissions, Reason, Receipt, Unconfirmed, WriteOptions, temp_name};

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
    pub fn of(metadata: &std::fs::Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Self { dev: metadata.dev(), ino: metadata.ino() }
        }
        #[cfg(not(unix))]
        {
            Self { len: metadata.len(), modified: metadata.modified().ok(), created: metadata.created().ok() }
        }
    }

    pub fn at(path: &Path) -> io::Result<Self> {
        std::fs::symlink_metadata(path).map(|m| Self::of(&m))
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
    Replace(Option<Replaced>),
    New,
}

struct Replaced {
    identity: Identity,
    metadata: std::fs::Metadata,
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

impl<'fs> Stage<'fs> {
    pub(crate) fn replace(options: WriteOptions<'fs>, path: &Path) -> io::Result<Self> {
        let target = resolve(options.links, path)?;
        let (dir, name) = split(&target, path)?;
        let replaced = std::fs::symlink_metadata(&target).ok().map(|metadata| Replaced { identity: Identity::of(&metadata), metadata });
        if replaced.as_ref().is_some_and(|r| r.metadata.is_file()) {
            check_writable(&target).map_err(|e| read_only_error(e, path))?;
        }
        let creation = if replaced.is_some() { NewFilePermissions::Private } else { options.new_file_permissions };
        let existed = replaced.is_some();
        let (tmp, file) = create_stage(options.fs, &dir, &name, creation).map_err(|e| write_error(e, path, &dir, existed))?;
        Ok(Self { options, path: path.to_path_buf(), target, dir, file, tmp, kind: Kind::Replace(replaced) })
    }

    pub(crate) fn create_new(options: WriteOptions<'fs>, path: &Path) -> io::Result<Self> {
        let target = resolve(options.links, path)?;
        let (dir, name) = split(&target, path)?;
        if std::fs::symlink_metadata(&target).is_ok() {
            return Err(write_error(io::ErrorKind::AlreadyExists.into(), path, &dir, true));
        }
        let (tmp, file) = create_stage(options.fs, &dir, &name, options.new_file_permissions).map_err(|e| write_error(e, path, &dir, false))?;
        Ok(Self { options, path: path.to_path_buf(), target, dir, file, tmp, kind: Kind::New })
    }

    pub fn file(&mut self) -> &mut File {
        &mut self.file
    }

    pub(crate) fn fill_and_publish(mut self, fill: impl FnOnce(&mut File) -> io::Result<()>) -> Receipt {
        match fill(&mut self.file) {
            Ok(()) => self.publish(),
            Err(e) => Receipt::Unchanged(write_error(e, &self.path, &self.dir, self.existed())),
        }
    }

    fn existed(&self) -> bool {
        matches!(self.kind, Kind::Replace(Some(_)))
    }

    pub fn publish(self) -> Receipt {
        let existed = self.existed();
        let Stage { options, path, target, dir, file, mut tmp, kind } = self;
        let fs = options.fs;
        let seal = |file: File| -> Result<Flush, Receipt> {
            if let Kind::Replace(Some(replaced)) = &kind
                && replaced.metadata.is_file()
            {
                if options.metadata == Metadata::Carry {
                    crate::meta::carry(&target, &file, &tmp.path);
                }
                keep_mode(fs, &file, &replaced.metadata).map_err(|e| Receipt::Unchanged(permissions_error(e, &path)))?;
            }
            fs.sync(&file).map_err(|e| Receipt::Unchanged(write_error(e, &path, &dir, existed)))
        };
        let mut content = match seal(file) {
            Ok(flush) => flush,
            Err(receipt) => return receipt,
        };
        match kind {
            Kind::Replace(replaced) => {
                let now = std::fs::symlink_metadata(&target).ok().map(|m| Identity::of(&m));
                if now != replaced.as_ref().map(|r| r.identity) {
                    return Receipt::Unchanged(changed_error(&path));
                }
                let windows_replace = replaced.as_ref().is_some_and(|r| r.metadata.is_file()) && options.metadata == Metadata::Carry;
                match swap(options, &tmp.path, &target, windows_replace) {
                    Ok(()) => tmp.armed = false,
                    Err(ReplaceFailure::Kept(e)) => return Receipt::Unchanged(rename_error(e, &path, existed)),
                    #[cfg(any(windows, test))]
                    Err(ReplaceFailure::Displaced { completion, restoration, original }) => {
                        return Receipt::Displaced { error: displaced_error(&path, &completion, &restoration, &original), original };
                    }
                }
            }
            Kind::New => match place_new(options, &tmp.path, &target) {
                Ok(Placed::Renamed) => tmp.armed = false,
                Ok(Placed::Linked) => {}
                Ok(Placed::Copied(flush)) => content = worse(content, flush),
                Err(e) => {
                    let taken = e.kind() == io::ErrorKind::AlreadyExists;
                    return Receipt::Unchanged(write_error(e, &path, &dir, taken));
                }
            },
        }
        drop(tmp);
        settle(fs, &dir, content)
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

fn worse(a: Flush, b: Flush) -> Flush {
    match (a, b) {
        (Flush::Full, other) | (other, Flush::Full) => other,
        (plain, Flush::Plain { .. }) => plain,
    }
}

fn settle(fs: &dyn AtomicFs, dir: &Path, content: Flush) -> Receipt {
    confirm(fs, content, fs.open_dir(dir))
}

pub fn confirm(fs: &dyn AtomicFs, content: Flush, folder: io::Result<File>) -> Receipt {
    let unconfirmed = |reason, error| Some(Unconfirmed { reason, error });
    let folder = match folder {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => unconfirmed(Reason::DirOpenDenied, e),
        Err(e) if dir_sync_unsupported(&e) => unconfirmed(Reason::DirSyncUnsupported, e),
        Err(e) => unconfirmed(Reason::DirSyncFailed, e),
        Ok(handle) => match fs.sync(&handle) {
            Ok(Flush::Full) => None,
            Ok(Flush::Plain { refused }) => unconfirmed(Reason::FullFlushRefused, refused),
            Err(e) if dir_sync_unsupported(&e) => unconfirmed(Reason::DirSyncUnsupported, e),
            Err(e) => unconfirmed(Reason::DirSyncFailed, e),
        },
    };
    match (folder, content) {
        (Some(u), _) => Receipt::VisibleUnconfirmed(u),
        (None, Flush::Plain { refused }) => Receipt::VisibleUnconfirmed(Unconfirmed { reason: Reason::FullFlushRefused, error: refused }),
        (None, Flush::Full) => Receipt::Durable,
    }
}

fn swap(options: WriteOptions<'_>, from: &Path, to: &Path, windows_replace: bool) -> Result<(), ReplaceFailure> {
    #[cfg(windows)]
    if windows_replace {
        let transient = |e: &ReplaceFailure| matches!(e, ReplaceFailure::Kept(e) if is_transient_lock(e));
        return retry_when(options.retry, transient, || crate::meta::replace_file(from, to));
    }
    #[cfg(not(windows))]
    let _ = windows_replace;
    retry_when(options.retry, is_transient_lock, || options.fs.rename(from, to)).map_err(ReplaceFailure::Kept)
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
    let copied = File::open(tmp).and_then(move |mut from| {
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
fn keep_mode(fs: &dyn AtomicFs, file: &File, replaced: &std::fs::Metadata) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let wanted = replaced.permissions();
    let staged = file.metadata()?;
    if staged.permissions().mode() & 0o7777 == wanted.mode() & 0o7777 {
        return Ok(());
    }
    let perms = adopt_group(fs, file, staged.gid(), replaced.gid(), wanted);
    fs.set_permissions(file, perms)
}

#[cfg(not(unix))]
fn keep_mode(_fs: &dyn AtomicFs, _file: &File, _replaced: &std::fs::Metadata) -> io::Result<()> {
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
fn check_writable(target: &Path) -> io::Result<()> {
    std::fs::OpenOptions::new().write(true).open(target).map(drop)
}

#[cfg(not(unix))]
fn check_writable(target: &Path) -> io::Result<()> {
    if std::fs::metadata(target)?.permissions().readonly() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the file is marked read-only"));
    }
    Ok(())
}

fn resolve(links: Links, path: &Path) -> io::Result<PathBuf> {
    match links {
        Links::Follow => resolve_links(path),
        Links::Replace => Ok(path.to_path_buf()),
    }
}

const MAX_LINKS: usize = 40;

fn resolve_links(path: &Path) -> io::Result<PathBuf> {
    let mut p = path.to_path_buf();
    for _ in 0..MAX_LINKS {
        match std::fs::symlink_metadata(&p) {
            Ok(m) if m.file_type().is_symlink() => {
                let to = std::fs::read_link(&p).map_err(|e| io::Error::new(e.kind(), format!("cannot save {}: {e}", path.display())))?;
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

#[cfg(any(windows, test))]
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
