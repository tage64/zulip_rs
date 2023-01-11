use anyhow::{bail, Context, Result};
use common_cache::CommonCache;
use derive_more::Deref;
use regex::{Regex, RegexBuilder};
use zulib::{message::*, stream::*};

#[derive(Debug, Deref)]
pub struct Client {
    #[deref]
    backend: zulib::Client,

    /// The currently selected stream, if any.
    selected_stream: Option<u64>,
    /// The currently selected topic if any. If `selected_stream` is `None` then must also
    /// `selected_topic` be `None`.
    selected_topic: Option<String>,

    /// A cache with recently used streams. Stream ids as keys and stream objects as values.
    stream_cache: CommonCache<u64, Stream>,
    /// A cache with recently read topics. Topic names as keys and stream id as value.
    topic_cache: CommonCache<String, u64>,
}

impl Client {
    pub fn new(rc: zulib::ZulipRc) -> Result<Self> {
        Ok(Self {
            backend: zulib::Client::new(rc)?,
            selected_stream: None,
            selected_topic: None,
            stream_cache: CommonCache::new(2, Some(128)),
            topic_cache: CommonCache::new(2, Some(512)),
        })
    }

    /// Get an iterator of streams matching a regex, in order with most
    /// commonly and recently used stream first. Only locally cached streams are considered.
    pub fn stream_search_in_cache<'a>(
        &'a self,
        re: &'a Regex,
    ) -> impl Iterator<Item = &'a Stream> + 'a {
        self.stream_cache
            .iter()
            .map(|(_id, stream)| stream)
            .filter(|stream| re.is_match(&stream.name))
    }

    /// Search for a stream by a regex. First considers the local cache and if that fails fetches
    /// all streams from the server. The found stream will be added to (or promoted in) the cache.
    async fn stream_search(&mut self, re: &Regex) -> Result<Option<&'_ Stream>> {
        if let Some(cache_idx) = self
            .stream_cache
            .find_first(|_, stream| re.is_match(&stream.name))
            .map(|x| x.index())
        {
            Ok(Some(cache_idx.get_value(&mut self.stream_cache)))
        } else {
            let streams = self.backend.get_streams(&Default::default()).await?;
            if let Some(stream) = streams.into_iter().filter(|x| re.is_match(&x.name)).next() {
                Ok(Some(
                    self.stream_cache
                        .insert(stream.stream_id, stream)
                        .peek_long()
                        .1,
                ))
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
            .topic_cache
            .find_first(|topic, &stream| stream == stream_id && re.is_match(topic))
            .map(|x| x.index())
        {
            Ok(Some(cache_idx.get_key_value(&mut self.topic_cache).0))
        } else {
            let topics = self.backend.get_topics_in_stream(stream_id).await?;
            if let Some(topic) = topics.into_iter().filter(|x| re.is_match(&x.name)).next() {
                Ok(Some(
                    self.topic_cache.insert(topic.name, stream_id).peek_long().0,
                ))
            } else {
                Ok(None)
            }
        }
    }

    /// Get a stream by id. Either from the local cache or fetched from the server. It'll be
    /// promoted in the local cache, so don't use this for a large number of automated calls if you
    /// don't want the user to think that this stream is used alot.
    async fn get_stream(&mut self, id: u64) -> Result<&Stream> {
        if let Some(cache_idx) = self.stream_cache.entry(&id).map(|x| x.index()) {
            Ok(cache_idx.get_value(&mut self.stream_cache))
        } else {
            Ok(self
                .stream_cache
                .insert(id, self.backend.get_stream_by_id(id).await?)
                .peek_long()
                .1)
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
    ) -> Result<GetMessagesResponse> {
        if regex_search {
            if let Some(narrows) = req.narrow.as_mut() {
                let mut found_stream = None;
                for Narrow {
                    operator, operand, ..
                } in narrows.iter_mut()
                {
                    if operator == "stream" {
                        if let Some(stream) = self.stream_search(&mk_regex(operand)?).await? {
                            *operand = stream.name.clone();
                            found_stream = Some(stream.stream_id);
                        } else {
                            bail!("No stream found matching: {operand}");
                        }
                    }
                }

                // Search for "topic" in the narrows.
                let mut found_topic = false;
                if let Some(stream) = found_stream.or(self.selected_stream) {
                    for Narrow {
                        operator, operand, ..
                    } in narrows.iter_mut()
                    {
                        if operator == "topic" {
                            if let Some(topic) =
                                self.topic_search(stream, &mk_regex(operand)?).await?
                            {
                                *operand = topic.clone();
                                found_topic = true;
                            } else {
                                bail!("No topic found matching: {operand}");
                            }
                        }
                    }
                }

                // If no stream/topic was narrowed and `global` is `false` and a topic or stream is
                // selected, add it to the list of narrows.
                if !global {
                    if found_stream.is_none() {
                        if let Some(selected_stream_id) = self.selected_stream {
                            narrows.push(Narrow {
                                operator: "stream".to_string(),
                                operand: self.get_stream(selected_stream_id).await?.name.clone(),
                                negated: false,
                            });
                        }
                    }
                    if !found_topic {
                        if let Some(selected_topic) = &self.selected_topic {
                            narrows.push(Narrow {
                                operator: "topic".to_string(),
                                operand: selected_topic.clone(),
                                negated: false,
                            });
                        }
                    }
                }
            }
        }
        Ok(self.backend.get_messages(req).await?)
    }
}

/// Create a case insensitive regex from a string.
fn mk_regex(pattern: &str) -> Result<Regex> {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .with_context(|| format!("Bad regular expression: {pattern}"))
}
