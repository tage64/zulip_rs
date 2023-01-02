use crate::message::*;
use crate::ZulipRc;
use reqwest::{Method, RequestBuilder};
use serde::{de::DeserializeOwned, Deserialize};

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

/// A response from the server in a unified format parameterized by the type of data we want to
/// retrieve.
///
/// This is primarily used for deserializing the response and should be converted to a `Result<T>`.
#[derive(serde::Serialize, Deserialize, Debug)]
#[serde(tag = "result", rename_all = "snake_case")]
enum Response<T> {
    Success(T),
    Error {
        code: String,
        msg: String,
        stream: Option<String>,
    },
}

impl<T> Response<T> {
    fn into_result(self) -> Result<T> {
        match self {
            Self::Success(x) => Ok(x),
            Self::Error { code, msg, stream } => Err(Error::Unsuccessful { code, msg, stream }),
        }
    }
}

/// Parse a JSON response from the server and convert it to a `Result<T>` where `T` is the type of
/// the requested data.
async fn parse_response<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
    let bytes = response.bytes().await?;
    serde_json::from_slice::<Response<T>>(&bytes)?.into_result()
}

pub struct Client {
    rc: ZulipRc,
}

impl Client {
    pub fn new(rc: ZulipRc) -> anyhow::Result<Self> {
        Ok(Self { rc })
    }
    pub async fn send_message(&self, req: SendMessageRequest) -> Result<SendMessageResponse> {
        let response = self
            .http_client(Method::POST, "/api/v1/messages")
            .form(&req)
            .send()
            .await?;
        parse_response(response).await
    }
    pub async fn get_messages(&self, req: GetMessagesRequest) -> Result<GetMessagesResponse> {
        let response = {
            let builder = self
                .http_client(Method::GET, "/api/v1/messages")
                .query(&req);
            log::debug!(
                "Request url: {}",
                builder.try_clone().unwrap().build().unwrap().url()
            );
            builder.send().await?
        };
        parse_response(response).await
    }
    pub async fn delete_message(&self, id: i64) -> Result<()> {
        let response = self
            .http_client(Method::DELETE, &format!("/api/v1/messages/{}", id))
            .send()
            .await?;
        parse_response(response).await
    }
    pub async fn edit_message(&self, req: EditMessageRequest) -> Result<()> {
        let response = self
            .http_client(
                Method::PATCH,
                &format!("/api/v1/messages/{}", req.message_id),
            )
            .form(&req)
            .send()
            .await?;
        parse_response(response).await
    }
    pub async fn add_emoji_reaction(&self, req: AddEmojiReactionRequest) -> Result<()> {
        let response = self
            .http_client(
                Method::POST,
                &format!("/api/v1/messages/{}/reactions", req.message_id),
            )
            .form(&req)
            .send()
            .await?;
        parse_response(response).await
    }
    pub async fn remove_emoji_reaction(&self, req: RemoveEmojiReactionRequest) -> Result<()> {
        let response = self
            .http_client(
                Method::DELETE,
                &format!("/api/v1/messages/{}/reactions", req.message_id),
            )
            .form(&req)
            .send()
            .await?;
        parse_response(response).await
    }
    fn http_client(&self, method: Method, endpoint: &str) -> RequestBuilder {
        let client = reqwest::Client::new();
        let url = format!("{}{}", &self.rc.site, endpoint);
        client
            .request(method, url)
            .basic_auth(&self.rc.email, Some(&self.rc.key))
            .header("application", "x-www-form-urlencoded")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::{
        Method::{DELETE, GET, POST},
        MockServer,
    };
    use std::net::SocketAddr;

    /// Creat a client for testing based on the socket address to the server.
    fn test_client(socket_addr: &SocketAddr) -> Client {
        Client::new(ZulipRc {
            email: "me@example.com".to_string(),
            key: "testkey".to_string(),
            site: format!("http://{socket_addr}"),
        })
        .unwrap()
    }
    #[tokio::test]
    async fn test_send_private_message() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/api/v1/messages");
            then.status(200)
                .body(r#"{"result": "success", "msg": "", "id": 123}"#);
        });
        let client = test_client(server.address());
        let req = SendMessageRequest::Private {
            to: "[8]".to_string(),
            content: "abc".to_string(),
        };
        let result = client.send_message(req).await;
        mock.assert();
        assert!(result.is_ok());
    }
    #[tokio::test]
    async fn test_send_stream_message() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/api/v1/messages");
            then.status(200)
                .body(r#"{"result": "success", "msg": "", "id": 123}"#);
        });
        let client = test_client(server.address());
        let req = SendMessageRequest::Stream {
            to: "[8]".to_string(),
            topic: "test".to_string(),
            content: "abc".to_string(),
        };
        let result = client.send_message(req).await;
        mock.assert();
        assert!(result.is_ok());
    }
    #[tokio::test]
    async fn test_get_messages() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/api/v1/messages");
            then.status(200).body(message_template());
        });
        let client = test_client(server.address());

        let req = GetMessagesRequest::new(0, 0);

        let result = client.get_messages(req).await;
        mock.assert();
        assert!(result.is_ok());
    }
    #[tokio::test]
    async fn test_delete_messages() {
        let server = MockServer::start();
        let id = 123;
        let mock = server.mock(|when, then| {
            when.method(DELETE).path(format!("/api/v1/messages/{}", id));
            then.status(200).body(r#"{"result": "success", "msg": ""}"#);
        });
        let client = test_client(server.address());
        let result = client.delete_message(id).await;
        mock.assert();
        assert!(result.is_ok());
    }
    fn message_template() -> String {
        r#"{
    "anchor": 21,
    "found_anchor": true,
    "found_newest": true,
    "messages": [
        {
            "avatar_url": "https://secure.gravatar.com/avatar/6d8cad0fd00256e7b40691d27ddfd466?d=identicon&version=1",
            "client": "populate_db",
            "content": "<p>Security experts agree that relational algorithms are an interesting new topic in the field of networking, and scholars concur.</p>",
            "content_type": "text/html",
            "display_recipient": [
                {
                    "email": "hamlet@zulip.com",
                    "full_name": "King Hamlet",
                    "id": 4,
                    "is_mirror_dummy": false
                },
                {
                    "email": "iago@zulip.com",
                    "full_name": "Iago",
                    "id": 5,
                    "is_mirror_dummy": false
                },
                {
                    "email": "prospero@zulip.com",
                    "full_name": "Prospero from The Tempest",
                    "id": 8,
                    "is_mirror_dummy": false
                }
            ],
            "flags": [
                "read"
            ],
            "id": 16,
            "is_me_message": false,
            "reactions": [],
            "recipient_id": 27,
            "sender_email": "hamlet@zulip.com",
            "sender_full_name": "King Hamlet",
            "sender_id": 4,
            "sender_realm_str": "zulip",
            "subject": "",
            "submessages": [],
            "timestamp": 1527921326,
            "topic_links": [],
            "type": "private"
        },
        {
            "avatar_url": "https://secure.gravatar.com/avatar/6d8cad0fd00256e7b40691d27ddfd466?d=identicon&version=1",
            "client": "populate_db",
            "content": "<p>Wait, is this from the frontend js code or backend python code</p>",
            "content_type": "text/html",
            "display_recipient": "Verona",
            "flags": [
                "read"
            ],
            "id": 21,
            "is_me_message": false,
            "reactions": [],
            "recipient_id": 20,
            "sender_email": "hamlet@zulip.com",
            "sender_full_name": "King Hamlet",
            "sender_id": 4,
            "sender_realm_str": "zulip",
            "stream_id": 5,
            "subject": "Verona3",
            "submessages": [],
            "timestamp": 1527939746,
            "topic_links": [],
            "type": "stream"
        }
    ],
    "msg": "",
    "result": "success"
}"#.to_string()
    }
}
