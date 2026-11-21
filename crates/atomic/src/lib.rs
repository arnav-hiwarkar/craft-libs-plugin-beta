#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

//! Crash-safe file publication for the Craft suite. Every write stages beside its destination,
//! publishes with one rename and returns a [`Receipt`] that never claims more durability than
//! the flushes delivered. See the README for the guarantees.

mod fs;
mod meta;
mod publish;
mod temp;
#[cfg(test)]
mod tests;

use std::fmt;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub use fs::{AtomicFs, Entry, EntryKind, Flush, RealFs};
pub use meta::ReplaceFailure;
pub use publish::{Identity, Stage};
pub use temp::{temp_name, temp_owner};

#[must_use]
#[derive(Debug)]
pub enum Receipt {
    Unchanged(io::Error),
    Displaced { error: io::Error, original: PathBuf },
    VisibleUnconfirmed(Unconfirmed),
    Durable,
}

impl Receipt {
    pub fn visible(self) -> io::Result<()> {
        self.published().map(drop)
    }

    pub fn published(self) -> io::Result<Option<Reasons>> {
        match self {
            Self::Durable => Ok(None),
            Self::VisibleUnconfirmed(u) => Ok(Some(u.reasons())),
            Self::Unchanged(error) | Self::Displaced { error, .. } => Err(error),
        }
    }

    pub fn and(self, other: Receipt) -> Receipt {
        match (self, other) {
            (failed @ (Self::Unchanged(_) | Self::Displaced { .. }), _) | (_, failed @ (Self::Unchanged(_) | Self::Displaced { .. })) => failed,
            (Self::VisibleUnconfirmed(a), Self::VisibleUnconfirmed(b)) => Self::VisibleUnconfirmed(a.merge(b)),
            (unconfirmed @ Self::VisibleUnconfirmed(_), Self::Durable) | (Self::Durable, unconfirmed) => unconfirmed,
        }
    }
}

#[derive(Debug)]
pub struct Unconfirmed {
    first: (Reason, io::Error),
    rest: Vec<(Reason, io::Error)>,
}

impl Unconfirmed {
    pub(crate) fn new(first: (Reason, io::Error), rest: Vec<(Reason, io::Error)>) -> Self {
        Self { first, rest }
    }

    pub fn causes(&self) -> impl Iterator<Item = (Reason, &io::Error)> {
        std::iter::once(&self.first).chain(&self.rest).map(|(r, e)| (*r, e))
    }

    pub fn reasons(&self) -> Reasons {
        self.causes().fold(Reasons(0), |set, (r, _)| set.with(r))
    }

    fn merge(mut self, other: Unconfirmed) -> Self {
        self.rest.push(other.first);
        self.rest.extend(other.rest);
        self
    }
}

impl fmt::Display for Unconfirmed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what: Vec<&str> = self.reasons().iter().map(Reason::what).collect();
        let errors: Vec<String> = self.causes().map(|(_, e)| e.to_string()).collect();
        write!(f, "the file is in place, but {}, so it may not survive a power loss or crash ({})", what.join(" and "), errors.join("; "))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    FullFlushRefused,
    DirOpenDenied,
    DirSyncUnsupported,
    DirSyncFailed,
    StageRemovalFailed,
}

impl Reason {
    pub const ALL: [Reason; 5] =
        [Self::FullFlushRefused, Self::DirOpenDenied, Self::DirSyncUnsupported, Self::DirSyncFailed, Self::StageRemovalFailed];

    pub const fn id(self) -> &'static str {
        match self {
            Self::FullFlushRefused => "full-flush-refused",
            Self::DirOpenDenied => "dir-open-denied",
            Self::DirSyncUnsupported => "dir-sync-unsupported",
            Self::DirSyncFailed => "dir-sync-failed",
            Self::StageRemovalFailed => "stage-removal-failed",
        }
    }

    const fn what(self) -> &'static str {
        match self {
            Self::FullFlushRefused => "the disk refused a full flush, so only an ordinary flush was done",
            Self::DirOpenDenied => "its folder could not be opened to flush it",
            Self::DirSyncUnsupported => "its file system cannot flush folders",
            Self::DirSyncFailed => "flushing its folder failed",
            Self::StageRemovalFailed => "its temporary file could not be removed",
        }
    }

    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reasons(u8);

impl Reasons {
    const fn with(self, reason: Reason) -> Self {
        Self(self.0 | reason.bit())
    }

    pub const fn contains(self, reason: Reason) -> bool {
        self.0 & reason.bit() != 0
    }

    pub const fn union(self, other: Reasons) -> Self {
        Self(self.0 | other.0)
    }

    pub fn iter(self) -> impl Iterator<Item = Reason> {
        Reason::ALL.into_iter().filter(move |r| self.contains(*r))
    }

    pub fn ids(self) -> Vec<&'static str> {
        self.iter().map(Reason::id).collect()
    }
}

impl fmt::Display for Reasons {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.ids().join(", "))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenameRetry {
    pub attempts: u32,
    pub first_delay_ms: u64,
}

impl RenameRetry {
    pub const NONE: Self = Self { attempts: 0, first_delay_ms: 0 };

    pub fn platform() -> Self {
        if cfg!(windows) { Self { attempts: 7, first_delay_ms: 10 } } else { Self::NONE }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NewFilePermissions {
    Private,
    #[default]
    PlatformDefault,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Links {
    #[default]
    Follow,
    Replace,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Metadata {
    #[default]
    Carry,
    ModeOnly,
}

#[derive(Clone, Copy)]
pub struct WriteOptions<'fs> {
    fs: &'fs dyn AtomicFs,
    retry: RenameRetry,
    new_file_permissions: NewFilePermissions,
    links: Links,
    metadata: Metadata,
}

impl Default for WriteOptions<'static> {
    fn default() -> Self {
        Self {
            fs: &RealFs,
            retry: RenameRetry::platform(),
            new_file_permissions: NewFilePermissions::default(),
            links: Links::default(),
            metadata: Metadata::default(),
        }
    }
}

impl<'fs> WriteOptions<'fs> {
    pub fn with_fs<'a>(self, fs: &'a dyn AtomicFs) -> WriteOptions<'a> {
        WriteOptions { fs, retry: self.retry, new_file_permissions: self.new_file_permissions, links: self.links, metadata: self.metadata }
    }

    pub const fn with_retry(mut self, retry: RenameRetry) -> Self {
        self.retry = retry;
        self
    }

    pub const fn with_new_file_permissions(mut self, permissions: NewFilePermissions) -> Self {
        self.new_file_permissions = permissions;
        self
    }

    pub const fn with_links(mut self, links: Links) -> Self {
        self.links = links;
        self
    }

    pub const fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn stage(self, path: &Path) -> io::Result<Stage<'fs>> {
        Stage::replace(self, path)
    }

    pub fn write(self, path: &Path, bytes: &[u8]) -> Receipt {
        self.write_with(path, |f| f.write_all(bytes))
    }

    pub fn write_with(self, path: &Path, fill: impl FnOnce(&mut File) -> io::Result<()>) -> Receipt {
        Stage::replace(self, path).map_or_else(Receipt::Unchanged, |stage| stage.fill_and_publish(fill))
    }

    pub fn write_new(self, path: &Path, bytes: &[u8]) -> Receipt {
        Stage::create_new(self, path).map_or_else(Receipt::Unchanged, |stage| stage.fill_and_publish(|f| f.write_all(bytes)))
    }
}

pub fn write(path: &Path, bytes: &[u8]) -> Receipt {
    WriteOptions::default().write(path, bytes)
}

pub fn write_new(path: &Path, bytes: &[u8]) -> Receipt {
    WriteOptions::default().write_new(path, bytes)
}
