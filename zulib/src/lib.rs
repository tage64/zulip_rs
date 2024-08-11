mod client;
pub mod message;
mod rc;
pub mod stream;

use std::str::FromStr;

pub use client::{Client, Error, Result};
pub use rc::ZulipRc;
use serde::{Deserialize, Serialize};

/// An identifier for E.G a stream or a message which both can be referenced by
/// an integer or a name.
#[derive(Serialize, Deserialize, Debug, Clone)]
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
