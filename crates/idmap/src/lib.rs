#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod check;
mod gate;
mod lock;
mod name;
mod resolve;

pub use check::{Finding, Registered, check, update};
pub use gate::{Edit, Request, regenerate, verify};
pub use lock::{EditError, Entry, FORMAT, Lock, MAX_ALIASES, MAX_ENTRIES, MAX_LOCK_BYTES, MAX_MIGRATIONS, MAX_TOMBSTONES, ParseError, Step};
pub use name::{ArgsHash, Class, MAX_CLASS_BYTES, MAX_NAME_BYTES, Name, NameError};
pub use resolve::{Migration, Resolved, Resolver};
