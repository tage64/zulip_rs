use anyhow::{bail, Context, Result};
use common_cache::CommonCache;
use derive_more::Deref;
use iter_tools::Itertools as _;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zulib::{message::*, stream::*};

#[derive(Debug, Deref)]
pub struct Client {
    #[deref]
    backend: zulib::Client,

    /// The currently selected stream, if any.
    selected_stream: Option<Stream>,
    /// The currently selected topic if any. If `selected_stream` is `None` then must also
    /// `selected_topic` be `None`.
    selected_topic: Option<String>,
    cache: Cache,
}

/// Some useful caches.
#[derive(Debug, Deserialize, Serialize, Clone)]
struct Cache {
    /// A cache with recently used streams. Stream ids as keys and stream objects as values.
    streams: CommonCache<u64, Stream>,
    /// A cache with recently read topics. Topic names as keys and stream id as value.
    topics: CommonCache<String, u64>,
}

impl Client {
    pub fn new(rc: zulib::ZulipRc) -> Result<Self> {
        Ok(Self {
            backend: zulib::Client::new(rc)?,
            selected_stream: None,
            selected_topic: None,
            cache: Cache {
                streams: CommonCache::new(2, Some(128)),
                topics: CommonCache::new(2, Some(512)),
            },
        })
    }

    /// Create a new client from a cache file.
    pub fn from_cache(cache_file_content: &str, rc: zulib::ZulipRc) -> Result<Self> {
        Ok(Self {
            cache: serde_json::from_str(cache_file_content)
                .context("Failed to parse cache file.")?,
            ..Self::new(rc)?
        })
    }

    /// Get the content of the cache file a(as it would be right now) as a string.
    pub fn mk_cache_file(&self) -> String {
        serde_json::to_string_pretty(&self.cache).unwrap()
    }

    /// Iterate over all streams in the cache, from most to least commonly/recently used.
    pub fn stream_cache_iter(&self) -> impl DoubleEndedIterator<Item = &Stream> {
        self.cache.streams.iter().map(|(_, x)| x)
    }

    /// Iterate over all topics in the cache, from most to least commonly/recently used.
    pub fn topic_cache_iter(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.cache.topics.iter().map(|(x, _)| x.as_str())
    }

    /// Clear the stream and topic cache.
    pub fn clear_cache(&mut self) {
        self.cache.streams.clear();
        self.cache.topics.clear();
    }

    /// Get an iterator of all streams (filtered by a `GetStreamsRequest`) in  order, with
    /// unsubscribed streams first, and then subscribed streams sorted by weekly trafic from lowest
    /// to highest. Nothing will be added to the cache.
    pub async fn get_streams(
        &self,
        req: &GetStreamsRequest,
    ) -> Result<impl Iterator<Item = Stream>> {
        let subscribed_streams: HashMap<u64, _> = self
            .get_subscribed_streams()
            .await?
            .into_iter()
            .map(|x| (x.stream_id, x))
            .collect();
        let (mut relevant_subscribed_streams, unsubscribed_streams) = self
            .backend
            .get_streams(req)
            .await?
            .into_iter()
            .partition::<Vec<_>, _>(|x| subscribed_streams.contains_key(&x.stream_id));
        relevant_subscribed_streams
            .sort_unstable_by_key(|x| subscribed_streams[&x.stream_id].stream_weekly_trafic);
        Ok(unsubscribed_streams
            .into_iter()
            .chain(relevant_subscribed_streams.into_iter()))
    }

    /// Get an iterator of streams matching a regex, in order with most
    /// commonly and recently used stream first. Only locally cached streams are considered.
    pub fn stream_search_in_cache<'a>(
        &'a self,
        re: &'a Regex,
    ) -> impl Iterator<Item = &'a Stream> + 'a {
        self.cache
            .streams
            .iter()
            .map(|(_id, stream)| stream)
            .filter(|stream| re.is_match(&stream.name))
    }

    /// Search for a stream by a regex. First considers the local cache and if that fails fetches
    /// subscribed streams from the server (ordered by weekly trafic). If that also failes, fetches
    /// all streams from the server. The found stream will be
    /// added to (or promoted in) the cache.
    async fn stream_search(
        &mut self,
        re: &Regex,
    ) -> Result<Option<common_cache::Entry<'_, u64, Stream>>> {
        if let Some(cache_idx) = self
            .cache
            .streams
            .find_first(|_, stream| re.is_match(&stream.name))
            .map(|x| x.index())
        {
            Ok(Some(cache_idx.entry(&mut self.cache.streams)))
        } else {
            let mut streams = self.backend.get_subscribed_streams().await?;
            streams.sort_unstable_by_key(|x| x.stream_weekly_trafic);
            if let Some(stream) = streams
                .into_iter()
                .map(|x| x.stream)
                .filter(|x| re.is_match(&x.name))
                .next()
            {
                Ok(Some(self.cache.streams.insert(stream.stream_id, stream)))
            } else if let Some(stream) = self
                .backend
                .get_streams(&GetStreamsRequest::default())
                .await?
                .into_iter()
                .filter(|x| re.is_match(&x.name))
                .next()
            {
                Ok(Some(self.cache.streams.insert(stream.stream_id, stream)))
            } else {
                Ok(None)
            }
        }
    }

    /// Search for a topic in a stream.
    ///
    /// First considers all cached topics for that particular stream, and then fetches all topics
    /// from the server and inserts the found topic into the cache.
    ///
    /// Returns the name of the topic.
    async fn topic_search(&mut self, stream_id: u64, re: &Regex) -> Result<Option<&'_ String>> {
        if let Some(cache_idx) = self
            .cache
            .topics
            .find_first(|topic, &stream| stream == stream_id && re.is_match(topic))
            .map(|x| x.index())
        {
            Ok(Some(cache_idx.get_key_value(&mut self.cache.topics).0))
        } else {
            let topics = self.backend.get_topics_in_stream(stream_id).await?;
            if let Some(topic) = topics.into_iter().filter(|x| re.is_match(&x.name)).next() {
                Ok(Some(
                    self.cache
                        .topics
                        .insert(topic.name, stream_id)
                        .peek_long()
                        .0,
                ))
            } else {
                Ok(None)
            }
        }
    }

    /// Get a stream by id. Either from the local cache or fetched from the server. It'll be
    /// promoted in the local cache, so don't use this for a large number of automated calls if you
    /// don't want the user to think that this stream is used alot.
    pub async fn get_stream(&mut self, id: u64) -> Result<&Stream> {
        if let Some(cache_idx) = self.cache.streams.entry(&id).map(|x| x.index()) {
            Ok(cache_idx.get_value(&mut self.cache.streams))
        } else {
            Ok(self
                .cache
                .streams
                .insert(id, self.backend.get_stream_by_id(id).await?)
                .peek_long()
                .1)
        }
    }

    /// Interpret the stream and topic fields of a narrow as regular expressions and replace them
    /// with their real names.
    ///
    /// The topic/stream will be searched for in the local cache.
    /// If no matching stream/topic is found in the cache, fetches all
    /// streams / all topics in the stream from the server.
    async fn unregex_narrow(&mut self, narrows: &mut [Narrow]) -> Result<()> {
        let mut found_stream = None;
        for Narrow {
            operator, operand, ..
        } in narrows.iter_mut()
        {
            if operator == "stream" {
                if let Some(mut stream_cache_entry) =
                    self.stream_search(&mk_regex(operand)?).await?
                {
                    let stream = stream_cache_entry.get_value();
                    *operand = stream.name.clone();
                    found_stream = Some(stream.stream_id);
                } else {
                    bail!("No stream found matching: {operand}");
                }
            }
        }

        // Search for "topic" in the narrows.
        if let Some(stream) = found_stream.or(self.selected_stream_id()) {
            for Narrow {
                operator, operand, ..
            } in narrows.iter_mut()
            {
                if operator == "topic" {
                    if let Some(topic) = self.topic_search(stream, &mk_regex(operand)?).await? {
                        *operand = topic.clone();
                    } else {
                        bail!("No topic found matching: {operand}");
                    }
                }
            }
        }
        Ok(())
    }

    /// Add the current stream/topic to a narrow if no stream/topic is specified in the narrow.
    fn narrow_to_current(&self, narrows: &mut Vec<Narrow>) {
        if !narrows.iter().any(|x| x.operator == "stream") {
            if let Some(selected_stream) = self.selected_stream.as_ref() {
                narrows.push(Narrow {
                    operator: "stream".to_string(),
                    operand: selected_stream.name.clone(),
                    negated: false,
                });
            }
        }
        if !narrows.iter().any(|x| x.operator == "topic") {
            if let Some(selected_topic) = &self.selected_topic {
                narrows.push(Narrow {
                    operator: "topic".to_string(),
                    operand: selected_topic.clone(),
                    negated: false,
                });
            }
        }
    }

    /// Get a list of all messages matching a query.
    ///
    /// If `regex_search` is `true`, the topic and/or stream narrows will be interpretted as regular
    /// expressions and searched in the local cache of recently read topics and streams. If no
    /// stream is found, all streams will be fetched from the server and searched. If a topic is
    /// not found, all topics for the searched stream (or currently selected stream) will be
    /// fetched from the server and searched.
    ///
    /// If the `global` flag is `false`, the straem and topic will default to the current ditto.
    pub async fn get_messages(
        &mut self,
        mut req: GetMessagesRequest,
        regex_search: bool,
        global: bool,
    ) -> Result<impl Iterator<Item = (String, Vec<ReceivedMessage>)>> {
        let narrows = req.range.narrow.get_or_insert(Default::default());
        if regex_search {
            self.unregex_narrow(narrows.as_mut_slice()).await?;
        }

        // If no stream/topic was narrowed and `global` is `false` and a topic or stream is
        // selected, add it to the list of narrows.
        if !global {
            self.narrow_to_current(narrows);
        }

        let messages = self.backend.get_messages(req).await?.messages;
        let grouped_messages = messages
            .into_iter()
            .into_grouping_map_by(|x| x.subject.clone())
            .collect::<Vec<_>>()
            .drain()
            .map(|(k, mut v)| {
                v.sort_unstable_by_key(|x| x.id);
                (k, v)
            })
            .sorted_unstable_by_key(|(_, msgs)| msgs[0].id);
        for (topic, messages) in grouped_messages.as_slice().iter() {
            if let Some(stream_id) = messages[0].stream_id {
                self.cache.topics.insert(topic.to_string(), stream_id);
            }
        }
        Ok(grouped_messages)
    }

    /// Update message flags for narrow.
    pub async fn update_message_flags_for_narrow(
        &mut self,
        mut req: UpdateMessageFlagsForNarrowRequest,
        regex: bool,
        global: bool,
    ) -> Result<UpdateMessageFlagsForNarrowResponse> {
        let narrows = req.range.narrow.get_or_insert(Default::default());
        if regex {
            self.unregex_narrow(narrows.as_mut_slice()).await?;
        }

        // If no stream/topic was narrowed and `global` is `false` and a topic or stream is
        // selected, add it to the list of narrows.
        if !global {
            self.narrow_to_current(narrows);
        }

        Ok(self.backend.update_message_flags_for_narrow(&req).await?)
    }

    /// Mark a bulk of messages read.
    pub async fn mark_read(
        &mut self,
        stream: Option<zulib::Identifier>,
        topic: Option<String>,
        regex: bool,
        global: bool,
    ) -> Result<()> {
        let stream_id = if let Some(stream) = stream {
            Some(match stream {
                zulib::Identifier::Id(x) => x,
                zulib::Identifier::Name(name) if regex => {
                    *self
                        .stream_search(&mk_regex(&name)?)
                        .await?
                        .context("Stream not found")?
                        .get_key_value()
                        .0
                }
                zulib::Identifier::Name(name) => self.backend.get_stream_id(&name).await?,
            })
        } else if global {
            self.selected_stream.as_ref().map(|x| x.stream_id)
        } else {
            None
        };
        if let Some(stream_id) = stream_id {
            if let Some(topic) = topic {
                let topic = if regex {
                    self.topic_search(stream_id, &mk_regex(&topic)?)
                        .await?
                        .context("No such topic found")?
                        .clone()
                } else {
                    topic
                };
                Ok(self.backend.mark_topic_as_read(stream_id, &topic).await?)
            } else {
                Ok(self.backend.mark_stream_as_read(stream_id).await?)
            }
        } else {
            Ok(self.backend.mark_all_as_read().await?)
        }
    }

    /// Select a stream by either a name or a regex for the name.
    ///
    /// If a regex is provided, the
    /// stream will first be searched for in the cache and then all streams will be fetched from
    /// the server. If a plain name is given, it will be checked that the stream indeed exists.
    ///
    /// Returns a reference to the newly selected stream.
    pub async fn select_stream(&mut self, name: &str, is_regex: bool) -> Result<&Stream> {
        if is_regex {
            let re = mk_regex(name)?;
            let stream = self
                .stream_search(&re)
                .await?
                .with_context(|| format!("No stream matching: {name}"))?
                .index();
            self.selected_stream = Some(stream.peek_value(&self.cache.streams).clone());
            Ok(stream.get_value(&mut self.cache.streams))
        } else {
            let id = self.backend.get_stream_id(name).await?;
            let stream = self.backend.get_stream_by_id(id).await?;
            self.selected_stream = Some(stream.clone());
            Ok(self.cache.streams.insert(id, stream).peek_long().1)
        }
    }

    /// Get a reference to the currently selected stream.
    pub fn selected_stream(&self) -> Option<&Stream> {
        self.selected_stream.as_ref()
    }

    /// Get the id of the currently selected stream if any.
    pub fn selected_stream_id(&self) -> Option<u64> {
        self.selected_stream.as_ref().map(|x| x.stream_id)
    }
}

/// Create a case insensitive regex from a string.
fn mk_regex(pattern: &str) -> Result<Regex> {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .with_context(|| format!("Bad regular expression: {pattern}"))
}
