use crate::Identifier;
use chrono::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
mod narrow;
pub use narrow::Narrow;

#[derive(Serialize, Deserialize, Debug)]
pub struct SendMessageResponse {
    pub id: u64,
    pub msg: String,
}

/// Send a message.
#[derive(Serialize, Deserialize, Debug, clap::Parser)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SendMessageRequest {
    /// Make a message to a stream.
    Stream {
        /// Stream ID (integer), or stream name.
        to: Identifier,
        topic: String,
        /// The content in markdown.
        content: String,
    },
    /// Make a private message.
    Private {
        /// Either a user ID (integer), or a user name.
        to: Identifier,
        /// The content as markdown.
        content: String,
    },
}

/// Type of anchor when retreiving messages.
///
/// `Anchor::Newest`, `Anchor::Oldest` and `Anchor::FirstUnread` are new in Zulip 3.0 (feature
/// level 1).
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Anchor {
    /// The most recent message.
    Newest,
    /// The oldest message.
    Oldest,
    /// The oldest unread message matching the query, if any; otherwise, the most recent message.
    FirstUnread,
    /// Integer message ID to anchor fetching of new messages.
    #[clap(skip)]
    MessageId(u64),
}

/// Specify a range of messages.
#[derive(Serialize, Debug, Clone, clap::Parser)]
pub struct MessageRange {
    /// Anchor the fetching of new messages.
    #[serde(serialize_with = "serialize_as_json_str")]
    #[clap(short = 'c', long, value_enum, default_value_t = Anchor::Newest)]
    pub anchor: Anchor,
    /// Whether a message with the specified ID matching the narrow should be included.
    ///
    /// New in Zulip 6.0 (feature level 155).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[clap(short, long)]
    pub include_anchor: Option<bool>,
    /// The number of messages with IDs less than the anchor to retrieve.
    #[clap(short = 'b', long, default_value_t = 10)]
    pub num_before: u64,
    /// The number of messages with IDs greater than the anchor to retrieve.
    #[clap(short = 'a', long, default_value_t = 10)]
    pub num_after: u64,
    /// The narrow (set of message filters) where you want to fetch the messages from.
    ///
    /// Note that many narrows, including all that lack a stream or streams operator, search the
    /// user's personal message history. See
    /// [here](https://zulip.com/help/search-for-messages#searching-shared-history) for details.
    /// For example, if you would like to fetch messages from all public streams instead of only
    /// the user's message history, then a specific narrow for messages sent to all public streams
    /// can be used: {"operator": "streams", "operand": "public"}.
    ///
    /// Newly created bot users are not usually subscribed to any streams, so bots using this API
    /// should either be subscribed to appropriate streams or use a shared history search narrow
    /// with this endpoint.
    #[serde(
        serialize_with = "serialize_as_json_str",
        skip_serializing_if = "Option::is_none"
    )]
    #[clap(value_parser = |s: &str| anyhow::Ok(Narrow::parse(s)))]
    pub narrow: Option<Vec<Narrow>>,
}

/// Get one or many messages.
#[derive(Serialize, Debug, Clone, clap::Parser)]
pub struct GetMessagesRequest {
    #[clap(flatten)]
    #[serde(flatten)]
    pub range: MessageRange,
    /// Whether the client supports computing gravatars URLs.
    ///
    /// If enabled, avatar_url will be
    /// included in the response only if there is a Zulip avatar, and will be null for users who
    /// are using gravatar as their avatar. This option significantly reduces the compressed size
    /// of user data, since gravatar URLs are long, random strings and thus do not compress well.
    ///
    /// The client_gravatar field should be set to true if clients can compute their own gravatars.
    #[clap(skip = true)]
    pub client_gravatar: bool,
    /// Convert the content from markdown to HTML on the server.
    ///
    /// If true, message content is returned in the rendered HTML format. If false, message content
    /// is returned in the raw Markdown-format text that user entered.
    #[clap(long)]
    pub apply_markdown: bool,
}

/// Information about updating a flag.
#[derive(Serialize, Debug, Clone, clap::Parser)]
pub struct UpdateFlag {
    /// Whether to add the flag or remove it.
    #[serde(rename = "op")]
    #[clap(value_enum)]
    operation: FlagOperation,
    /// The flag that should be added/removed.
    #[clap(value_enum)]
    flag: EditableFlag,
}

/// Add or remove personal message flags like read and starred on a list of messages.
#[derive(Serialize, Debug, Clone, clap::Parser)]
pub struct UpdateMessageFlagsRequest {
    #[clap(flatten)]
    #[serde(flatten)]
    pub update: UpdateFlag,
    /// A vector containing the IDs of the target messages.
    #[serde(serialize_with = "serialize_as_json_str")]
    messages: Vec<u64>,
}

/// Add or remove personal message flags like read and starred on a range of messages restrained by
/// a narrow.
#[derive(Serialize, Debug, Clone, clap::Parser)]
pub struct UpdateMessageFlagsForNarrowRequest {
    #[clap(flatten)]
    #[serde(flatten)]
    pub update: UpdateFlag,
    #[clap(flatten)]
    #[serde(flatten)]
    pub range: MessageRange,
}

#[derive(Serialize, Debug, Clone, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum FlagOperation {
    Add,
    Remove,
}

/// A flag that can be edited by the user.
#[derive(Serialize, Deserialize, Debug, Clone, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum EditableFlag {
    /// Whether the user has read the message. Messages start out unread (except for messages the
    /// user themself sent using a non-API client) and can later be marked as read.
    Read,
    /// Whether the user has starred this message.
    Starred,
    /// Whether the user has collapsed this message.
    Collapsed,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Flag {
    Editable(EditableFlag),
    /// Whether the current user was mentioned by this message, either directly or via a user
    /// group. Cannot be changed by the user directly, but can change if the message is edited to
    /// add/remove a mention of the current user.
    Mentioned,
    /// Whether this message contained wildcard mention like @**all**. Cannot be changed by the
    /// user directly, but can change if the message is edited to add/remove a wildcard mention.
    WildcardMentioned,
    /// Whether the message contains any of the current user's configured alert words. Cannot be
    /// changed by the user directly, but can change if the message is edited to add/remove one of
    /// the current user's alert words.
    HasAlertWord,
    /// True for messages that the user did not receive at the time they were sent but later was
    /// added to the user's history (E.g. because they starred or reacted to a message sent to a
    /// public stream before they subscribed to that stream). Cannot be changed by the user
    /// directly.
    Historical,
}

impl MessageRange {
    pub fn new(num_before: u64, num_after: u64) -> Self {
        Self {
            anchor: Anchor::Newest,
            num_before,
            num_after,
            narrow: None,
            include_anchor: None,
        }
    }

    pub fn anchor(&mut self, anchor: Anchor) -> &mut Self {
        self.anchor = anchor;
        self
    }
    pub fn narrow(&mut self, narrow: Vec<Narrow>) -> &mut Self {
        self.narrow = Some(narrow);
        self
    }
}

impl GetMessagesRequest {
    pub fn new(range: MessageRange) -> Self {
        Self {
            range,
            apply_markdown: true,
            client_gravatar: true,
        }
    }
}

impl Serialize for Anchor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Newest => serializer.serialize_unit_variant("newest", 0, "newest"),
            Self::Oldest => serializer.serialize_unit_variant("oldest", 1, "oldest"),
            Self::FirstUnread => {
                serializer.serialize_unit_variant("first_unread", 2, "first_unread")
            }
            Self::MessageId(x) => serializer.serialize_u64(*x),
        }
    }
}

/// The response of a get_messages request.
#[derive(Serialize, Deserialize, Debug)]
pub struct GetMessagesResponse {
    /// The same anchor specified in the request (or the computed one, if
    /// `GetMessagesRequest::anchor` was set to `Anchor::FirstUnread`).
    pub anchor: u64,
    /// Whether the messages list includes the very newest messages matching the narrow (used by
    /// clients that paginate their requests to decide whether there are more messages to fetch).
    pub found_newest: bool,
    /// Whether the messages list includes the very oldest messages matching the narrow (used by
    /// clients that paginate their requests to decide whether there are more messages to fetch).
    pub found_oldest: Option<bool>,
    /// Whether the anchor message is included in the response. If the message with the ID
    /// specified in the request does not exist, did not match the narrow, or was excluded via
    /// include_anchor=false, this will be false.
    pub found_anchor: bool,
    /// Whether the message history was limited due to plan restrictions.
    ///
    /// This flag is set to true only when the oldest messages(found_oldest) matching the narrow is fetched.
    pub history_limited: Option<bool>,
    /// The retreived messages.
    pub messages: Vec<ReceivedMessage>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReceivedMessage {
    /// The unique message ID. Messages should always be displayed sorted by ID.
    pub id: u64,
    /// The UNIX timestamp for when the message was sent, in UTC seconds.
    #[serde(with = "chrono::serde::ts_seconds")]
    pub timestamp: DateTime<Utc>,
    /// The content/body of the message.
    pub content: String,
    /// The HTTP content_type for the message content. This will be text/html or text/x-markdown,
    /// depending on whether apply_markdown was set.
    pub content_type: String,
    /// The URL of the user's avatar. Can be null only if client_gravatar was passed, which means
    /// that the user has not uploaded an avatar in Zulip, and the client should compute the
    /// gravatar URL by hashing the user's email address itself for this user.
    pub avatar_url: Option<String>,
    /// A Zulip "client" string, describing what Zulip client sent the message.
    pub client: String,
    /// Data of the recipient of the message.
    pub display_recipient: DisplayRecipient,
    /// The time for when the message was last edited.
    ///
    /// `None` if the message has never been edited.
    #[serde(default, deserialize_with = "deserialize_timestamp_to_option")]
    pub last_edit_timestamp: Option<DateTime<Utc>>,
    /// A list of edits, with each element documenting the changes in a previous edit made to
    /// the the message, ordered chronologically from most recent to least recent edit.
    pub edit_history: Option<Vec<EditHistory>>,
    /// Whether the message is a /me status message.
    pub is_me_message: bool,
    /// Reactions to the message.
    pub reactions: Vec<Reaction>,
    /// A unique ID for the set of users receiving the message (either a stream or group of users).
    /// Useful primarily for hashing.
    pub recipient_id: u64,
    /// The Zulip display email address of the message's sender.
    pub sender_email: String,
    /// The full name of the message's sender.
    pub sender_full_name: String,
    /// The user ID of the message's sender.
    pub sender_id: u64,
    /// A string identifier for the realm the sender is in. Unique only within the context of a
    /// given Zulip server.
    ///
    /// E.g. on example.zulip.com, this will be example.
    pub sender_realm_str: String,
    /// Only present for stream messages; the ID of the stream.
    pub stream_id: Option<u64>,
    pub subject: String,
    pub r#type: MessageType,
    /// The user's message flags for the message.
    pub flags: Vec<Flag>,
    /// (Only present if keyword search was included among the narrow parameters.)
    /// HTML content of a queried message that matches the narrow, with <span class="highlight">
    /// elements wrapping the matches for the search keywords.
    pub match_content: Option<String>,
    /// (Only present if keyword search was included among the narrow parameters.)
    /// HTML-escaped topic of a queried message that matches the narrow, with <span
    /// class="highlight"> elements wrapping the matches for the search keywords.
    pub match_subject: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    Private,
    Stream,
}

/// Data of the recipient of a message.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum DisplayRecipient {
    /// The name of the stream the message was sent to.
    Stream(String),
    PrivateMessage(Vec<DisplayRecipientPrivateMessage>),
    BasicRicipientData(serde_json::Map<String, serde_json::Value>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DisplayRecipientPrivateMessage {
    pub id: i64,
    pub email: String,
    pub full_name: String,
    pub is_mirror_dummy: bool,
}

/// The response for a update message flag request.
#[derive(Serialize, Deserialize, Debug)]
pub struct UpdateMessageFlagsResponse {
    /// A vector with the IDs of the modified messages.
    messages: Vec<u64>,
}

/// The response for a update message flags for narrow request.
#[derive(Serialize, Deserialize, Debug)]
pub struct UpdateMessageFlagsForNarrowResponse {
    /// The number of messages that were within the update range (at most num_before + 1 +
    /// num_after).
    pub processed_count: u64,
    /// The number of messages where the flag's value was changed (at most processed_count).
    pub update_count: u64,
    /// The ID of the oldest message within the update range, or `None` if the range was empty.
    pub first_processed_id: Option<u64>,
    /// The ID of the newest message within the update range, or `None` if the range was empty.
    pub last_processed_id: Option<u64>,
    /// Whether the update range reached backward far enough to include very oldest message
    /// matching the narrow (used by clients doing a bulk update to decide whether to issue another
    /// request anchored at first_processed_id).
    pub found_oldest: bool,
    /// Whether the update range reached forward far enough to include very oldest message matching
    /// the narrow (used by clients doing a bulk update to decide whether to issue another request
    /// anchored at last_processed_id).
    pub found_newest: bool,
}

/// A historical edit of a message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EditHistory {
    /// The time for the edit.
    #[serde(with = "chrono::serde::ts_seconds")]
    pub timestamp: DateTime<Utc>,
    /// The ID of the user that made the edit.
    ///
    /// Will be `None` only for edit history events predating March 2017.
    /// Clients can display edit history events where this `None` as modified by either the sender
    /// (for content edits) or an unknown user (for topic edits).
    pub user_id: Option<u64>,
    /// The content of the message immediately prior to this edit event.
    ///
    /// Only present if message's content was edited.
    pub prev_content: Option<String>,
    /// The rendered HTML representation of prev_content.
    ///
    /// Only present if message's content was edited.
    pub prev_rendered_content: Option<String>,
    /// The Markdown processor version number for the message immediately prior to this edit event.
    ///
    /// Only present if message's content was edited.
    pub prev_rendered_content_version: Option<u64>,
    /// The ID of the stream containing the message immediately after this edit event.
    ///
    /// Only present if message's stream was edited.
    pub stream: Option<u64>,
    /// The stream ID of the message immediately prior to this edit event.
    ///
    /// Only present if message's stream was edited.
    pub prev_stream: Option<u64>,
    /// The topic of the message immediately after this edit event.
    ///
    /// Only present if message's topic was edited.
    ///
    /// New in Zulip 5.0 (feature level 118).
    pub topic: Option<String>,
    /// The topic of the message immediately prior to this edit event.
    ///
    /// Only present if message's topic was edited.
    #[serde(alias = "prev_subject")]
    pub prev_topic: Option<String>,
}

/// A reaction to a message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Reaction {
    pub emoji_code: String,
    pub emoji_name: String,
    pub reaction_type: String,
    pub user_id: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EditMessageRequest {
    #[serde(skip_serializing)]
    pub(crate) message_id: i64,
    topic: Option<String>,
    propagate_mode: PropagateMode,
    send_notification_to_old_thread: bool,
    send_notification_to_new_thread: bool,
    content: Option<String>,
    stream_id: Option<i64>,
}

impl EditMessageRequest {
    pub fn new(message_id: i64) -> Self {
        Self {
            message_id,
            topic: None,
            propagate_mode: PropagateMode::ChangeOne,
            send_notification_to_new_thread: true,
            send_notification_to_old_thread: true,
            content: None,
            stream_id: None,
        }
    }
    pub fn topic(&mut self, topic: &str) -> &mut Self {
        self.topic = Some(topic.to_string());
        self
    }
    pub fn propagate_mode(&mut self, propagate_mode: PropagateMode) -> &mut Self {
        self.propagate_mode = propagate_mode;
        self
    }
    pub fn send_notification_to_old_thread(&mut self, is_send: bool) -> &mut Self {
        self.send_notification_to_old_thread = is_send;
        self
    }
    pub fn send_notification_to_new_thread(&mut self, is_send: bool) -> &mut Self {
        self.send_notification_to_new_thread = is_send;
        self
    }
    pub fn content(&mut self, content: &str) -> &mut Self {
        self.content = Some(content.to_string());
        self
    }
    pub fn stream_id(&mut self, stream_id: i64) -> &mut Self {
        self.stream_id = Some(stream_id);
        self
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AddEmojiReactionRequest {
    #[serde(skip_serializing)]
    pub(crate) message_id: i64,
    emoji_name: String,
    emoji_code: Option<String>,
    reaction_type: Option<ReactionType>,
}

impl AddEmojiReactionRequest {
    pub fn new(message_id: i64, emoji_name: &str) -> Self {
        Self {
            message_id,
            emoji_name: emoji_name.to_string(),
            emoji_code: None,
            reaction_type: None,
        }
    }
    pub fn emoji_code(&mut self, emoji_code: &str) -> &mut Self {
        self.emoji_code = Some(emoji_code.to_string());
        self
    }
    pub fn reaction_type(&mut self, reaction_type: ReactionType) -> &mut Self {
        self.reaction_type = Some(reaction_type);
        self
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RemoveEmojiReactionRequest {
    pub(crate) message_id: i64,
    emoji_name: Option<String>,
    emoji_code: Option<String>,
    reaction_type: Option<ReactionType>,
}

impl RemoveEmojiReactionRequest {
    pub fn new(message_id: i64) -> Self {
        Self {
            message_id,
            emoji_name: None,
            emoji_code: None,
            reaction_type: None,
        }
    }
    pub fn emoji_name(&mut self, emoji_name: &str) -> &mut Self {
        self.emoji_name = Some(emoji_name.to_string());
        self
    }
    pub fn emoji_code(&mut self, emoji_code: &str) -> &mut Self {
        self.emoji_code = Some(emoji_code.to_string());
        self
    }
    pub fn reaction_type(&mut self, reaction_type: ReactionType) -> &mut Self {
        self.reaction_type = Some(reaction_type);
        self
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ReactionType {
    UnicodeEmoji,
    RealmEmoji,
    ZulipExtraEmoji,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum PropagateMode {
    ChangeOne,
    ChangeAll,
    ChangeLater,
}

fn deserialize_timestamp_to_option<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<DateTime<Utc>>, D::Error> {
    chrono::serde::ts_seconds::deserialize(deserializer).map(Option::Some)
}

fn serialize_as_json_str<S: Serializer, T: Serialize>(
    value: &T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let ser_json =
        serde_json::to_string(value).map_err(|e| <S::Error as serde::ser::Error>::custom(e))?;
    // This is a real hack: If the serialized json happens to be a string, we don't want to include
    // the quotes.
    let relevant_str = if ser_json.starts_with('"') && ser_json.ends_with('"') {
        &ser_json[1..ser_json.len() - 1]
    } else {
        &ser_json
    };
    serializer.serialize_str(&relevant_str)
}
