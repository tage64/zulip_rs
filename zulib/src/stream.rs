//! Types for requests and responses about streams.
use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::ops::Deref;

/// Get a list of streams.
#[derive(Serialize, Deserialize, clap::Parser, Debug, Clone)]
pub struct GetStreamsRequest {
    /// Toggle inclusion of all public streams.
    #[clap(short = 'p', long="no-include-public", action = clap::ArgAction::SetFalse)]
    pub include_public: bool,
    /// Include all web-public streams.
    #[clap(short = 'w', long)]
    pub include_web_public: bool,
    /// Toggle inclusion of all streams that the user is subscribed to.
    #[clap(short = 's', long="no-include-subscribed", action = clap::ArgAction::SetFalse)]
    pub include_subscribed: bool,
    /// Include all active streams. The user must have administrative privileges to use this
    /// parameter.
    #[clap(short = 'a', long)]
    pub include_active: bool,
    /// Include all default streams for the user's realm.
    #[clap(short = 'd', long)]
    pub include_default: bool,
    /// If the user is a bot, include all streams that the bot's owner is subscribed to.
    #[clap(short = 'o', long)]
    pub include_owner_subscribed: bool,
}

impl Default for GetStreamsRequest {
    fn default() -> Self {
        Self {
            include_public: true,
            include_web_public: false,
            include_subscribed: true,
            include_active: false,
            include_default: false,
            include_owner_subscribed: false,
        }
    }
}

/// A wrapper around the response from the get_streams request.
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct GetStreamsResponse {
    pub streams: Vec<Stream>,
}

/// A wrapper around the response from get_stream_by_id.
#[derive(Deserialize, Debug)]
pub(crate) struct GetStreamResponse {
    pub stream: Stream,
}

/// Information about a stream.
///
/// Can be fetched with `crate::Client::get_streams`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Stream {
    /// The id of the stream.
    pub stream_id: u64,
    /// The name of a stream.
    pub name: String,
    /// The description of the stream in text/markdown format, intended to be used to prepopulate
    /// UI for editing a stream's description.
    pub description: String,
    /// The description of the stream rendered as HTML, intended to be used when displaying the
    /// stream description in a UI.
    ///
    /// One should use the standard Zulip rendered_markdown CSS when displaying this content so
    /// that emoji, LaTeX, and other syntax work correctly. And any client-side security logic for
    /// user-generated message content should be applied when displaying this HTML as though it
    /// were the body of a Zulip message.
    pub rendered_description: String,
    /// The time when the stream was created.
    #[serde(with = "chrono::serde::ts_seconds")]
    pub date_created: DateTime<Utc>,
    /// Specifies whether the stream is private or not. Only people who have been invited can
    /// access a private stream.
    pub invite_only: bool,
    /// Policy for which users can post messages to the stream.
    pub stream_post_policy: StreamPostPolicy,
    /// Number of days that messages sent to this stream will be stored before being automatically
    /// deleted by the message retention policy.
    ///
    /// There are two special values:
    /// - `None`, the default, means the stream will inherit the organization level setting.
    /// - -1 encodes retaining messages in this stream forever.
    pub message_retention_days: Option<i64>,
    /// Whether the history of the stream is public to its subscribers.
    pub history_public_to_subscribers: bool,
    /// The ID of the first message in the stream.
    ///
    /// Intended to help clients determine whether they need to display UI like the "more topics"
    /// widget that would suggest the stream has older history that can be accessed.
    /// `None` is used for streams with no message history.
    pub first_message_id: Option<u64>,
    /// ID of the user group whose members are allowed to unsubscribe others from the stream.
    ///
    /// New in Zulip 6.0 (feature level 142), will be `None` if not present.
    pub can_remove_subscribers: Option<u64>,
}

/// Information about a stream the user is subscribed to.
///
/// Can be requested with `crate::Client::get_subscribed_streams`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Subscription {
    /// Information about the stream, not specific to the user.
    #[serde(flatten)]
    pub stream: Stream,
    /// A boolean specifying whether desktop notifications are enabled for the given stream.
    ///
    /// A `None` value means the value of this setting should be inherited from the user-level
    /// default setting, enable_stream_desktop_notifications, for this stream.
    pub desktop_notifications: Option<bool>,
    /// A boolean specifying whether email notifications are enabled for the given stream.
    ///
    /// A `None` value means the value of this setting should be inherited from the user-level
    /// default setting, enable_stream_email_notifications, for this stream.
    pub email_notifications: Option<bool>,
    /// A boolean specifying whether wildcard mentions trigger notifications as though they were
    /// personal mentions in this stream.
    ///
    /// A `None` value means the value of this setting should be inherited from the user-level
    /// default setting, wildcard_mentions_notify, for this stream.
    pub wildcard_mentions_notify: Option<bool>,
    /// A boolean specifying whether push notifications are enabled for the given stream.
    ///
    /// A null value means the value of this setting should be inherited from the user-level
    /// default setting, enable_stream_push_notifications, for this stream.
    pub push_notifications: Option<bool>,
    /// A boolean specifying whether audible notifications are enabled for the given stream.
    ///
    /// A `None` value means the value of this setting should be inherited from the user-level
    /// default setting, enable_stream_audible_notifications, for this stream.
    pub audible_notifications: Option<bool>,
    /// A boolean specifying whether the given stream has been pinned to the top.
    pub pin_to_top: bool,
    /// Email address of the given stream, used for sending emails to the stream.
    pub email_address: String,
    /// Whether the user has muted the stream. Muted streams do not count towards your total unread
    /// count and do not show up in All messages view (previously known as Home view).
    pub is_muted: bool,
    /// Whether the stream has been configured to allow unauthenticated access to its message
    /// history from the web.
    pub is_web_public: bool,
    /// The user's personal color for the stream.
    pub color: String,
    /// The average number of messages sent to the stream in recent weeks, rounded to the nearest
    /// integer.
    ///
    /// `None` means the stream was recently created and there is insufficient data to estimate the
    /// average traffic.
    pub stream_weekly_trafic: Option<u64>,
}

impl Deref for Subscription {
    type Target = Stream;
    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}

#[derive(Deserialize, Debug)]
pub(crate) struct GetSubscribedStreamsResponse {
    pub subscriptions: Vec<Subscription>,
}

/// Policy levels for posting messages to a stream.
#[derive(Serialize_repr, Deserialize_repr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StreamPostPolicy {
    /// Any user can post.
    AnyUser = 1,
    /// Only administrators can post.
    OnlyAdministrators = 2,
    /// Only full members can post.
    OnlyFullMembers = 3,
    /// Only moderators can post.
    OnlyModerators = 4,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub struct Topic {
    /// The message ID of the last message sent to this topic.
    pub max_id: u64,
    /// The name of the topic.
    pub name: String,
}

#[derive(Deserialize, Debug)]
pub(crate) struct TopicsInStreamResponse {
    pub topics: Vec<Topic>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct StreamId {
    pub stream_id: u64,
}
