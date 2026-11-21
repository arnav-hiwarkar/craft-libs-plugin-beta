use std::fmt;

use sha2::{Digest, Sha256};

pub const MAX_NAME_BYTES: usize = 256;
pub const MAX_CLASS_BYTES: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name(String);

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("the name is empty")]
    Empty,
    #[error("the name is {0} bytes, over the {MAX_NAME_BYTES}-byte limit")]
    TooLong(usize),
    #[error("the name holds {0:?}, outside printable ASCII without quotes, backslashes or spaces")]
    Character(char),
    #[error("the class {0:?} is not 1 to {MAX_CLASS_BYTES} lowercase letters, digits or dashes starting with a letter")]
    Class(String),
    #[error("the argument hash {0:?} is not 16 lowercase hex digits")]
    Hash(String),
}

impl Name {
    pub fn new(text: &str) -> Result<Name, NameError> {
        if text.is_empty() {
            return Err(NameError::Empty);
        }
        if text.len() > MAX_NAME_BYTES {
            return Err(NameError::TooLong(text.len()));
        }
        if let Some(c) = text.chars().find(|c| !c.is_ascii_graphic() || *c == '"' || *c == '\\') {
            return Err(NameError::Character(c));
        }
        Ok(Name(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Class(String);

impl Class {
    pub fn new(text: &str) -> Result<Class, NameError> {
        let mut chars = text.chars();
        let starts = chars.next().is_some_and(|c| c.is_ascii_lowercase());
        if !starts || text.len() > MAX_CLASS_BYTES || !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err(NameError::Class(text.chars().take(MAX_CLASS_BYTES + 1).collect()));
        }
        Ok(Class(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Class {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArgsHash([u8; 8]);

impl ArgsHash {
    pub fn of_keys<'a>(keys: impl IntoIterator<Item = &'a str>) -> ArgsHash {
        let mut keys: Vec<&str> = keys.into_iter().collect();
        keys.sort_unstable();
        keys.dedup();
        let mut hasher = Sha256::new();
        for key in keys {
            hasher.update(key.as_bytes());
            hasher.update(b"\n");
        }
        let digest = hasher.finalize();
        let mut bytes = [0u8; 8];
        for (out, byte) in bytes.iter_mut().zip(digest.iter()) {
            *out = *byte;
        }
        ArgsHash(bytes)
    }

    pub fn parse(text: &str) -> Result<ArgsHash, NameError> {
        let bad = || NameError::Hash(text.chars().take(17).collect());
        if text.len() != 16 || !text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err(bad());
        }
        let mut bytes = [0u8; 8];
        for (i, out) in bytes.iter_mut().enumerate() {
            let pair = text.get(i * 2..i * 2 + 2).ok_or_else(bad)?;
            *out = u8::from_str_radix(pair, 16).map_err(|_| bad())?;
        }
        Ok(ArgsHash(bytes))
    }
}

impl fmt::Display for ArgsHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}
