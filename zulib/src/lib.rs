mod client;
pub mod message;
mod rc;
pub mod stream;

pub use client::{Client, Error, Result};
pub use rc::ZulipRc;
use std::str::FromStr;

/// An identifier for E.G a stream or a message which both can be referenced by an integer or a
/// name.
#[derive(serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum Identifier {
    Id(u64),
    Name(String),
}

impl From<String> for Identifier {
    fn from(s: String) -> Self {
        u64::from_str(&s).map(Self::Id).unwrap_or(Self::Name(s))
    }
}

impl From<&str> for Identifier {
    fn from(s: &str) -> Self {
        u64::from_str(s)
            .map(Self::Id)
            .unwrap_or_else(|_| Self::Name(s.to_string()))
    }
}
