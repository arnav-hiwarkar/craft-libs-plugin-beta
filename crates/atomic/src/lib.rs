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

pub use fs::{AtomicFs, Flush, RealFs, retry_rename, sync_file};
pub use meta::{copy_created, copy_xattrs};
pub use publish::{Identity, Stage, confirm};
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
        match self {
            Self::Durable | Self::VisibleUnconfirmed(_) => Ok(()),
            Self::Unchanged(error) | Self::Displaced { error, .. } => Err(error),
        }
    }
    pub fn durable(self) -> io::Result<()> {
        match self {
            Self::Durable => Ok(()),
            Self::VisibleUnconfirmed(u) => Err(io::Error::new(u.error.kind(), u.to_string())),
            Self::Unchanged(error) | Self::Displaced { error, .. } => Err(error),
        }
    }
}

#[derive(Debug)]
pub struct Unconfirmed {
    pub reason: Reason,
    pub error: io::Error,
}

impl fmt::Display for Unconfirmed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self.reason {
            Reason::FullFlushRefused => "the disk refused a full flush, so only an ordinary flush was done",
            Reason::DirOpenDenied => "its folder could not be opened to flush it",
            Reason::DirSyncUnsupported => "its file system cannot flush folders",
            Reason::DirSyncFailed => "flushing its folder failed",
        };
        write!(f, "the file is in place, but {what}, so it may not survive a power loss or crash ({})", self.error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    FullFlushRefused,
    DirOpenDenied,
    DirSyncUnsupported,
    DirSyncFailed,
}

impl Reason {
    pub const fn id(self) -> &'static str {
        match self {
            Self::FullFlushRefused => "full-flush-refused",
            Self::DirOpenDenied => "dir-open-denied",
            Self::DirSyncUnsupported => "dir-sync-unsupported",
            Self::DirSyncFailed => "dir-sync-failed",
        }
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
