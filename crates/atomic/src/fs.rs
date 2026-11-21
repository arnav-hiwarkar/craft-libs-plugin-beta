use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use crate::meta::ReplaceFailure;
use crate::{Identity, NewFilePermissions, RenameRetry};

pub trait AtomicFs: Sync {
    fn entry(&self, path: &Path) -> io::Result<Entry>;
    fn read_link(&self, path: &Path) -> io::Result<PathBuf>;
    fn open_read(&self, path: &Path) -> io::Result<File>;
    fn open_write(&self, path: &Path) -> io::Result<File>;
    fn create_new(&self, path: &Path, permissions: NewFilePermissions) -> io::Result<File>;
    fn sync(&self, file: &File) -> io::Result<Flush>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn replace_existing(&self, from: &Path, to: &Path) -> Result<(), ReplaceFailure>;
    fn rename_noreplace(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove(&self, path: &Path) -> io::Result<()>;
    fn open_dir(&self, dir: &Path) -> io::Result<File>;
    fn copy_acl(&self, from: &Path, to: &Path);
    fn set_permissions(&self, file: &File, perms: std::fs::Permissions) -> io::Result<()>;
    #[cfg(unix)]
    fn set_group(&self, file: &File, gid: u32) -> io::Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Link,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub kind: EntryKind,
    pub identity: Identity,
    #[cfg(unix)]
    pub mode: u32,
    #[cfg(unix)]
    pub gid: u32,
    #[cfg(not(unix))]
    pub readonly: bool,
}

impl Entry {
    pub fn of(metadata: &std::fs::Metadata) -> Self {
        let kind = match metadata.file_type() {
            t if t.is_symlink() => EntryKind::Link,
            t if t.is_file() => EntryKind::File,
            _ => EntryKind::Other,
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Self { kind, identity: Identity::new(metadata.dev(), metadata.ino()), mode: metadata.mode(), gid: metadata.gid() }
        }
        #[cfg(not(unix))]
        {
            Self {
                kind,
                identity: Identity::new(metadata.len(), metadata.modified().ok(), metadata.created().ok()),
                readonly: metadata.permissions().readonly(),
            }
        }
    }
}

#[derive(Debug)]
pub enum Flush {
    Full,
    Plain { refused: io::Error },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RealFs;

impl AtomicFs for RealFs {
    fn entry(&self, path: &Path) -> io::Result<Entry> {
        std::fs::symlink_metadata(path).map(|m| Entry::of(&m))
    }

    fn read_link(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::read_link(path)
    }

    fn open_read(&self, path: &Path) -> io::Result<File> {
        File::open(path)
    }

    fn open_write(&self, path: &Path) -> io::Result<File> {
        OpenOptions::new().write(true).open(path)
    }

    fn create_new(&self, path: &Path, permissions: NewFilePermissions) -> io::Result<File> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(match permissions {
                NewFilePermissions::Private => 0o600,
                NewFilePermissions::PlatformDefault => 0o666,
            });
        }
        #[cfg(not(unix))]
        let _ = permissions;
        options.open(path)
    }

    fn sync(&self, file: &File) -> io::Result<Flush> {
        sync_file(file)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    fn replace_existing(&self, from: &Path, to: &Path) -> Result<(), ReplaceFailure> {
        #[cfg(windows)]
        {
            crate::meta::replace_file(from, to)
        }
        #[cfg(not(windows))]
        {
            std::fs::rename(from, to).map_err(ReplaceFailure::Kept)
        }
    }

    fn rename_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
        native_rename_noreplace(from, to)
    }

    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::hard_link(from, to)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn open_dir(&self, dir: &Path) -> io::Result<File> {
        open_dir_for_sync(dir)
    }

    fn copy_acl(&self, from: &Path, to: &Path) {
        crate::meta::copy_acl(from, to);
    }

    fn set_permissions(&self, file: &File, perms: std::fs::Permissions) -> io::Result<()> {
        file.set_permissions(perms)
    }

    #[cfg(unix)]
    fn set_group(&self, file: &File, gid: u32) -> io::Result<()> {
        std::os::unix::fs::fchown(file, None, Some(gid))
    }
}

fn sync_file(file: &File) -> io::Result<Flush> {
    #[cfg(target_vendor = "apple")]
    {
        or_plain_fsync(file.sync_all(), || rustix::fs::fsync(file).map_err(io::Error::from))
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        file.sync_all().map(|()| Flush::Full)
    }
}

#[cfg(any(target_vendor = "apple", test))]
pub(crate) fn or_plain_fsync(full: io::Result<()>, fsync: impl FnOnce() -> io::Result<()>) -> io::Result<Flush> {
    match full {
        Ok(()) => Ok(Flush::Full),
        Err(refused) if full_flush_refused(&refused) => fsync().map(|()| Flush::Plain { refused }),
        Err(e) => Err(e),
    }
}

#[cfg(any(target_vendor = "apple", test))]
fn full_flush_refused(e: &io::Error) -> bool {
    const ENOTTY: i32 = 25;
    const ENOTSUP: i32 = 45;
    cfg!(target_vendor = "apple") && matches!(e.raw_os_error(), Some(ENOTSUP | ENOTTY))
}

#[cfg(unix)]
fn open_dir_for_sync(dir: &Path) -> io::Result<File> {
    File::open(dir)
}

#[cfg(windows)]
fn open_dir_for_sync(dir: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_ADD_FILE: u32 = 0x0002;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    OpenOptions::new().access_mode(FILE_ADD_FILE).custom_flags(FILE_FLAG_BACKUP_SEMANTICS).open(dir)
}

#[cfg(not(any(unix, windows)))]
fn open_dir_for_sync(_dir: &Path) -> io::Result<File> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "this platform cannot open a folder to flush it"))
}

pub(crate) fn dir_sync_unsupported(e: &io::Error) -> bool {
    if e.kind() == io::ErrorKind::Unsupported {
        return true;
    }
    if cfg!(windows) {
        return matches!(e.raw_os_error(), Some(1 | 50));
    }
    const EINVAL: i32 = 22;
    let unsupported: &[i32] = if cfg!(any(target_os = "linux", target_os = "android")) {
        &[EINVAL, 95]
    } else if cfg!(target_vendor = "apple") {
        &[EINVAL, 45, 102, 25]
    } else if cfg!(any(target_os = "freebsd", target_os = "openbsd", target_os = "netbsd", target_os = "dragonfly")) {
        &[EINVAL, 45]
    } else if cfg!(unix) {
        &[EINVAL]
    } else {
        &[]
    };
    e.raw_os_error().is_some_and(|code| unsupported.contains(&code))
}

#[cfg(windows)]
fn native_rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    let (Some(from), Some(to)) = (from.to_str(), to.to_str()) else {
        return Err(io::ErrorKind::Unsupported.into());
    };
    if from.contains('\0') || to.contains('\0') {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "file path contains a NUL character"));
    }
    winsafe::MoveFile(from, to).map_err(|error| io::Error::from_raw_os_error(error.raw() as i32))
}

#[cfg(all(unix, any(target_os = "linux", target_os = "android", target_os = "redox", target_vendor = "apple")))]
fn native_rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    rustix::fs::renameat_with(rustix::fs::CWD, from, rustix::fs::CWD, to, rustix::fs::RenameFlags::NOREPLACE).map_err(io::Error::from)
}

#[cfg(not(any(windows, all(unix, any(target_os = "linux", target_os = "android", target_os = "redox", target_vendor = "apple")))))]
fn native_rename_noreplace(_from: &Path, _to: &Path) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "this platform has no exclusive rename"))
}

pub(crate) fn rename_noreplace_unsupported(error: &io::Error) -> bool {
    #[cfg(unix)]
    {
        error.kind() == io::ErrorKind::Unsupported
            || matches!(error.raw_os_error(), Some(code) if code == rustix::io::Errno::INVAL.raw_os_error() || code == rustix::io::Errno::NOSYS.raw_os_error())
    }
    #[cfg(not(unix))]
    {
        error.kind() == io::ErrorKind::Unsupported
    }
}

const MAX_RETRY_DELAY_MS: u64 = 200;

pub(crate) fn retry_when<E>(retry: RenameRetry, transient: impl Fn(&E) -> bool, mut op: impl FnMut() -> Result<(), E>) -> Result<(), E> {
    let mut delay = retry.first_delay_ms.min(MAX_RETRY_DELAY_MS);
    let mut left = retry.attempts;
    loop {
        match op() {
            Err(e) if transient(&e) && left > 0 => {
                left -= 1;
                sleep_ms(delay);
                delay = delay.saturating_mul(2).min(MAX_RETRY_DELAY_MS);
            }
            r => return r,
        }
    }
}

pub(crate) fn is_transient_lock(e: &io::Error) -> bool {
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;
    e.kind() == io::ErrorKind::PermissionDenied || (cfg!(windows) && matches!(e.raw_os_error(), Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)))
}

#[cfg(not(target_arch = "wasm32"))]
fn sleep_ms(ms: u64) {
    if ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}
#[cfg(target_arch = "wasm32")]
fn sleep_ms(_ms: u64) {}
