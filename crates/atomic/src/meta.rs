use std::fs::File;
use std::io;
use std::path::Path;

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
const MAX_XATTR_NAMES: usize = 1 << 20;
#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
const MAX_XATTR_VALUE: usize = 16 << 20;
#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
const XATTR_CHUNK: usize = 4 << 10;

pub(crate) fn carry(fs: &dyn crate::AtomicFs, replaced: &Path, stage: &File, stage_path: &Path) {
    let Ok(old) = fs.open_read(replaced) else { return };
    let _ = copy_xattrs(&old, stage);
    let _ = copy_created(&old, stage);
    fs.copy_acl(replaced, stage_path);
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
fn copy_xattrs(from: &File, to: &File) -> io::Result<()> {
    use rustix::fs::{XattrFlags, fsetxattr};
    for name in list_xattrs(from)?.split(|b| *b == 0) {
        if name.is_empty() || name == b"com.apple.decmpfs" {
            continue;
        }
        let Some(value) = read_xattr(from, name) else { continue };
        let _ = fsetxattr(to, name, &value, XattrFlags::empty());
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
fn copy_xattrs(_from: &File, _to: &File) -> io::Result<()> {
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
fn list_xattrs(file: &File) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; XATTR_CHUNK];
    loop {
        match rustix::fs::flistxattr(file, buf.as_mut_slice()) {
            Ok(len) => {
                buf.truncate(len.min(buf.len()));
                return Ok(buf);
            }
            Err(e) if e == rustix::io::Errno::RANGE && buf.len() < MAX_XATTR_NAMES => {
                buf.resize(buf.len().saturating_mul(4).min(MAX_XATTR_NAMES), 0);
            }
            Err(e) => return Err(e.into()),
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
fn read_xattr(file: &File, name: &[u8]) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; XATTR_CHUNK];
    loop {
        match rustix::fs::fgetxattr(file, name, buf.as_mut_slice()) {
            Ok(len) => {
                buf.truncate(len.min(buf.len()));
                return Some(buf);
            }
            Err(e) if e == rustix::io::Errno::RANGE && buf.len() < MAX_XATTR_VALUE => {
                buf.resize(buf.len().saturating_mul(4).min(MAX_XATTR_VALUE), 0);
            }
            Err(_) => return None,
        }
    }
}

fn copy_created(from: &File, to: &File) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::FileTimesExt;
        let created = from.metadata()?.created()?;
        to.set_times(std::fs::FileTimes::new().set_created(created))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (from, to);
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn copy_acl(from: &Path, to: &Path) {
    if let Ok(entries) = exacl::getfacl(from, None)
        && !entries.is_empty()
    {
        let _ = exacl::setfacl(&[to], &entries, None);
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn copy_acl(_from: &Path, _to: &Path) {}

#[derive(Debug)]
pub enum ReplaceFailure {
    Kept(io::Error),
    Displaced { completion: io::Error, restoration: io::Error, original: std::path::PathBuf },
}

#[cfg(any(windows, test))]
pub(crate) fn recover_moved_replacement(
    complete: impl FnOnce() -> io::Result<()>,
    restore: impl FnOnce() -> io::Result<()>,
    original: std::path::PathBuf,
) -> Result<(), ReplaceFailure> {
    let Err(completion) = complete() else { return Ok(()) };
    match restore() {
        Ok(()) => Err(ReplaceFailure::Kept(completion)),
        Err(restoration) => Err(ReplaceFailure::Displaced { completion, restoration, original }),
    }
}

#[cfg(windows)]
pub(crate) fn replace_file(from: &Path, to: &Path) -> Result<(), ReplaceFailure> {
    use winsafe::co::REPLACEFILE;
    const MOVE_REPLACEMENT_2: i32 = 1177;
    let Some(backup) = backup_path(to) else { return std::fs::rename(from, to).map_err(ReplaceFailure::Kept) };
    let (Some(replaced), Some(replacement), Some(backup_str)) = (to.to_str(), from.to_str(), backup.to_str()) else {
        return std::fs::rename(from, to).map_err(ReplaceFailure::Kept);
    };
    let flags = REPLACEFILE::IGNORE_MERGE_ERRORS | REPLACEFILE::IGNORE_ACL_ERRORS;
    match winsafe::ReplaceFile(replaced, replacement, Some(backup_str), flags) {
        Ok(()) => {
            let _ = std::fs::remove_file(&backup);
            Ok(())
        }
        Err(e) if e.raw() as i32 == MOVE_REPLACEMENT_2 => {
            let recovered = recover_moved_replacement(|| std::fs::rename(from, to), || std::fs::rename(&backup, to), backup.clone());
            if recovered.is_ok() {
                let _ = std::fs::remove_file(&backup);
            }
            recovered
        }
        Err(_) => {
            let _ = std::fs::remove_file(&backup);
            std::fs::rename(from, to).map_err(ReplaceFailure::Kept)
        }
    }
}

#[cfg(windows)]
fn backup_path(to: &Path) -> Option<std::path::PathBuf> {
    let name = to.file_name()?.to_string_lossy().into_owned();
    let dir = match to.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    Some(dir.join(crate::temp_name(&name)))
}
