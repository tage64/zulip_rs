pub mod message;
mod raw_client;
mod rc;
pub mod stream;

pub use raw_client::RawClient;
pub use rc::ZulipRc;
use std::str::FromStr;

/// An error that might occur when making a reqwest to the Zulip server.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// A response from the server that the requested operation failed.
    ///
    /// This is usually a recoverable error. It might for instance occur when one tries to send a
    /// message to a user that does not exist.
    #[error("Unsuccessful: {code}, {msg}")]
    Unsuccessful {
        /// This is a short string acting as identifier for the error.
        ///
        /// It is named "code" in the API so we keep that name although it  might be a bit
        /// confusing.
        code: String,
        /// A message from the server regarding the error.
        msg: String,
        /// A stream related to the error. Not applicable in most cases.
        stream: Option<String>,
    },

    /// The parsing of the JSON data in the response body (from the server) failed.
    #[error("Failed to parse response body")]
    BadResponse(#[from] serde_json::Error),

    /// A network/HTTP error from the reqwest crate.
    #[error("Network/HTTP error")]
    Network(#[from] reqwest::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

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
