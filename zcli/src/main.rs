use std::ops::ControlFlow;

use anyhow::*;
use chrono_humanize::HumanTime;
use clap::Parser as _;
use zcli::Client;
use zulib::message::*;
use zulib::stream::*;

#[derive(clap::Parser)]
#[command(author, version, about)]
struct Args {
    #[command(subcommand)]
    command: CommandOrRepl,
}

#[derive(clap::Subcommand)]
enum CommandOrRepl {
    #[clap(flatten)]
    Command(Command),
    /// Run a repl interactively instead of providing a command directly.
    #[clap(short_flag = 'i')]
    Repl,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Print various things, like messages or streams.
    #[clap(subcommand)]
    Ls(Ls),
    #[clap(subcommand)]
    Send(SendMessageRequest),
    /// Mark all messages, possibly in a specific stream or topic, as read.
    MarkRead {
        /// Specify a stream in which to mark as read. Otherwise all of your
        /// messages will be read.
        ///
        /// The stream should be a stream id or a name, which can optionally be
        /// a regex.
        stream: Option<zulib::Identifier>,
        /// Specify a topic to mark as read.
        topic: Option<String>,
        /// Interpret the "stream" and "topic" names as regular expressions and
        /// try to find the corresponding stream/topic.
        ///
        /// This will first consider the most recently/commonly used
        /// estreams/topics, and then fetch streams or topics from the
        /// server. If not found in the local cache, only topics
        /// from the searched or else currently selected stream will be
        /// considered.
        #[clap(short, long)]
        regex: bool,
    },
    UpdateFlags {
        #[clap(flatten)]
        req: UpdateMessageFlagsForNarrowRequest,
        /// Interpret the "stream" and "topic" names as regular expressions and
        /// try to find the corresponding stream/topic.
        ///
        /// This will first consider the most recently/commonly used
        /// estreams/topics, and then fetch streams or topics from the
        /// server. If not found in the local cache, only topics
        /// from the searched or else currently selected stream will be
        /// considered.
        #[clap(short, long)]
        regex: bool,
    },
    /// Clear the caches of streams and topics.
    ClearCache,
}

#[derive(clap::Subcommand)]
enum Ls {
    #[clap(short_flag = 'm')]
    Messages {
        #[clap(flatten)]
        req: GetMessagesRequest,
        /// Interpret the "stream" and "topic" queries as regular expressions
        /// and try to find the corresponding stream/topic.
        ///
        /// This will first consider the most recently/commonly used
        /// estreams/topics, and then fetch streams or topics from the
        /// server. If not found in the local cache, only topics
        /// from the searched or else currently selected stream will be
        /// considered.
        #[clap(short, long)]
        regex: bool,
        /// Only print the name of all topics and the timestamp of their last
        /// message.
        #[clap(short, long)]
        only_topics: bool,
    },
    #[clap(short_flag = 's')]
    Streams(GetStreamsRequest),
    /// Get all subscribed streams.
    #[clap(short_flag = 'b')]
    Subscribed,
    /// Get all topics for a stream.
    #[clap(short_flag = 't')]
    Topics {
        /// The name or id of the stream.
        stream: zulib::Identifier,
    },
    /// List streams or topics in the cache.
    Cache {
        /// Whether to show the stream or topic cache.
        #[clap(value_enum)]
        kind: StreamOrTopic,
    },
}

#[derive(clap::ValueEnum, Clone, Copy, PartialEq, Eq, Debug)]
enum StreamOrTopic {
    Stream,
    Topic,
}

impl Ls {
    async fn run(self, client: &mut Client) -> Result<()> {
        match self {
            Ls::Messages {
                req,
                regex,
                only_topics,
            } => {
                for (topic, messages) in client.get_messages(req, regex, false).await? {
                    if only_topics {
                        println!(
                            "{}: {topic}: {}, {} messages",
                            match &messages.as_slice()[0].display_recipient {
                                DisplayRecipient::Stream(s) => s.as_str(),
                                _ => "private",
                            },
                            HumanTime::from(messages.as_slice()[0].timestamp),
                            messages.as_slice().len()
                        );
                    } else {
                        println!("\n----------");
                        println!("{topic}:");
                        for message in messages {
                            println!(
                                "  - {} -- {}",
                                message.sender_full_name,
                                HumanTime::from(message.timestamp)
                            );
                            println!(
                                "{}\n",
                                textwrap::fill(
                                    &message.content,
                                    textwrap::Options::with_termwidth()
                                        .initial_indent("    ")
                                        .subsequent_indent("    ")
                                )
                            );
                        }
                    }
                }
            }
            Ls::Streams(req) => {
                let streams = client.get_streams(&req).await?;
                for stream in streams {
                    println!("{} -- {}", stream.name, stream.description);
                }
            }
            Ls::Subscribed => {
                let subscriptions = client.get_subscribed_streams().await?;
                for subscription in subscriptions {
                    println!(
                        "{} -- {}",
                        subscription.stream.name,
                        if subscription.is_muted {
                            "Muted"
                        } else {
                            "Unmuted"
                        }
                    );
                }
            }
            Ls::Topics { stream } => {
                let stream_id = match stream {
                    zulib::Identifier::Id(x) => x,
                    zulib::Identifier::Name(x) => client.get_stream_id(&x).await?,
                };
                let mut topics = client.get_topics_in_stream(stream_id).await?;
                topics.sort();
                for Topic { name, .. } in topics {
                    println!("{name}");
                }
            }
            Ls::Cache {
                kind: StreamOrTopic::Stream,
            } => {
                for stream in client.stream_cache_iter().rev() {
                    println!("{}", stream.name);
                }
            }
            Ls::Cache {
                kind: StreamOrTopic::Topic,
            } => {
                for topic in client.topic_cache_iter().rev() {
                    println!("{topic}");
                }
            }
        }
        Ok(())
    }
}

impl Command {
    async fn run(self, client: &mut Client) -> Result<()> {
        match self {
            Command::Ls(x) => x.run(client).await?,
            Command::Send(req) => {
                println!("Sending: {req:?}");
            }
            Command::ClearCache => client.clear_cache(),
            Command::MarkRead {
                stream,
                topic,
                regex,
            } => client.mark_read(stream, topic, regex, false).await?,
            Command::UpdateFlags { req, regex } => {
                client
                    .update_message_flags_for_narrow(req, regex, false)
                    .await?;
            }
        }
        Ok(())
    }
}

impl CommandOrRepl {
    async fn run(self, client: &mut Client) -> Result<()> {
        match self {
            Self::Command(x) => x.run(client).await,
            Self::Repl => {
                clap_repl::run_repl(prompt_str, |x, y| Box::pin(ReplCommand::run(x, y)), client)
                    .await
            }
        }
    }
}

#[derive(clap::Subcommand)]
enum ReplCommand {
    #[clap(flatten)]
    Command(Command),
    /// Quit the repl.
    #[clap(visible_aliases = &["q", "exit"])]
    Quit,
    /// Select a stream.
    #[clap(visible_aliases=&["ss"])]
    SelectStream {
        /// The name of the stream to select. Can be a regular expression.
        stream: String,
        /// Don't interpret the stream name as a regular expression.
        #[clap(short = 's', long)]
        no_regex: bool,
    },
}

impl ReplCommand {
    async fn run(self, client: &mut Client) -> Result<ControlFlow<(), ()>> {
        match self {
            Self::Command(x) => x.run(client).await.map(ControlFlow::Continue),
            Self::Quit => Ok(ControlFlow::Break(())),
            Self::SelectStream { stream, no_regex } => {
                let stream = client.select_stream(&stream, !no_regex).await?;
                println!("Selected stream {}", stream.name);
                Ok(ControlFlow::Continue(()))
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    flexi_logger::Logger::try_with_str("info, zulip=debug, zulib=warn")
        .unwrap()
        .start()?;

    let zuliprc_path = dirs::home_dir()
        .context("No home dir (in which to find .zuliprc) found.")?
        .join(".zuliprc");
    let zuliprc = zulib::ZulipRc::parse_from_str(
        &std::fs::read_to_string(&zuliprc_path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    anyhow!(
                        "A .zuliprc file corresponding to your account on a particular Zulip \
                         server can be downloaded via Web or Desktop applications connected to \
                         that server. In recent versions this can be found in your Personal \
                         settings in the Account & privacy section, under API key as 'Show/change \
                         your API key'."
                    )
                } else {
                    anyhow!(e)
                }
            })
            .with_context(|| format!("Failed to read .zuliprc at {}", zuliprc_path.display()))?,
    )?;

    let cache_file_path: Option<_> = dirs::cache_dir().map(|x| x.join("zcli.json"));
    let cache_file_content: Option<String> = cache_file_path
        .as_ref()
        .and_then(
            |cache_file_path| match std::fs::read_to_string(cache_file_path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                x => Some(x.with_context(|| {
                    format!("Failed to read cache file at {}", cache_file_path.display())
                })),
            },
        )
        .transpose()?;
    let mut client = if let Some(cache) = cache_file_content {
        Client::from_cache(&cache, zuliprc)?
    } else {
        Client::new(zuliprc)?
    };

    args.command.run(&mut client).await?;
    if let Some(cache_file_path) = cache_file_path {
        std::fs::write(cache_file_path, client.mk_cache_file())?;
    }
    Ok(())
}

/// Generate a prompt string.
fn prompt_str(client: &mut Client) -> String {
    if let Some(stream) = client.selected_stream() {
        format!("(zcli)->{}: ", stream.name)
    } else {
        "(zcli) ".to_string()
    }
}
