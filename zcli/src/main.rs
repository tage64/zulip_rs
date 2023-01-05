use anyhow::*;
use chrono_humanize::HumanTime;
use clap::Parser as _;
use iter_tools::Itertools as _;
use std::ops::ControlFlow;
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
}

#[derive(clap::Subcommand)]
enum Ls {
    #[clap(short_flag = 'm')]
    Messages {
        #[clap(flatten)]
        req: GetMessagesRequest,
        /// Only print the name of all topics and the timestamp of their last message.
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
}

impl Ls {
    async fn run(self, z_client: &zulib::Client) -> Result<()> {
        match self {
            Ls::Messages {
                mut req,
                only_topics,
            } => {
                if let Some(narrows) = req.narrow.as_mut() {
                    let mut stream = None;
                    for Narrow {
                        operator, operand, ..
                    } in narrows.iter_mut()
                    {
                        if operator == "stream" {
                            let re = regex::RegexBuilder::new(operand)
                                .case_insensitive(true)
                                .build()?;
                            let streams = z_client.get_streams(&Default::default()).await?;
                            let mut matching_streams =
                                streams.into_iter().filter(|x| re.is_match(&x.name));
                            let Some(stream1) = matching_streams.next() else {
                                bail!("No stream matching the regular expression {operand}");
                            };
                            if let Some(stream2) = matching_streams.next() {
                                bail!(
                                    "Multiple streams matched: {} and {} ...",
                                    stream1.name,
                                    stream2.name,
                                );
                            }
                            *operand = stream1.name.clone();
                            stream = Some(stream1.stream_id);
                        } else if operator == "topic" {
                            let re = regex::RegexBuilder::new(operand)
                                .case_insensitive(true)
                                .build()?;
                            let topics = if let Some(stream) = stream {
                                z_client.get_topics_in_stream(stream).await?
                            } else {
                                // Fetch all topics from all streams.
                                let streams = z_client.get_streams(&Default::default()).await?;
                                let mut topics = Vec::new();
                                for stream in streams {
                                    topics.extend(
                                        z_client.get_topics_in_stream(stream.stream_id).await?,
                                    );
                                }
                                topics
                            };
                            let mut matching_topics =
                                topics.into_iter().filter(|x| re.is_match(&x.name));
                            let Some(topic1) = matching_topics.next() else {
                                bail!("No topic matching the regular expression {operand}");
                            };
                            if let Some(topic2) = matching_topics.next() {
                                bail!(
                                    "Multiple topics matched: {} and {} ...",
                                    topic1.name,
                                    topic2.name,
                                );
                            }
                            *operand = topic1.name;
                        }
                    }
                }
                let messages = z_client.get_messages(req).await?.messages;
                for (topic, messages) in messages
                    .into_iter()
                    .into_grouping_map_by(|x| x.subject.clone())
                    .collect::<Vec<_>>()
                    .drain()
                    .map(|(k, v)| (k, v.into_iter().sorted_unstable_by_key(|x| x.id)))
                    .sorted_unstable_by_key(|(_, msgs)| msgs.as_slice()[0].id)
                {
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
                let mut streams = z_client.get_streams(&req).await?;
                streams.sort_unstable_by_key(|x| x.stream_weekly_trafic);
                for stream in streams {
                    println!("{} -- {}", stream.name, stream.description);
                }
            }
            Ls::Subscribed => {
                let subscriptions = z_client.get_subscribed_streams().await?;
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
                    zulib::Identifier::Name(x) => z_client.get_stream_id(&x).await?,
                };
                let mut topics = z_client.get_topics_in_stream(stream_id).await?;
                topics.sort();
                for Topic { name, .. } in topics {
                    println!("{name}");
                }
            }
        }
        Ok(())
    }
}

impl Command {
    async fn run(self, z_client: &zulib::Client) -> Result<()> {
        match self {
            Command::Ls(x) => x.run(z_client).await?,
            Command::Send(req) => {
                println!("Sending: {req:?}");
            }
        }
        Ok(())
    }
}

impl CommandOrRepl {
    async fn run(self, mut z_client: zulib::Client) -> Result<()> {
        match self {
            Self::Command(x) => x.run(&z_client).await,
            Self::Repl => {
                clap_repl::run_repl(
                    "(zcli) ",
                    |x, y| Box::pin(ReplCommand::run(x, y)),
                    &mut z_client,
                )
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
}

impl ReplCommand {
    async fn run(self, z_client: &mut zulib::Client) -> Result<ControlFlow<(), ()>> {
        match self {
            Self::Command(x) => x.run(z_client).await.map(ControlFlow::Continue),
            Self::Quit => Ok(ControlFlow::Break(())),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    flexi_logger::Logger::try_with_str("info, zulip=debug")
        .unwrap()
        .start()?;

    let zuliprc = zulib::ZulipRc::parse_from_str(&std::fs::read_to_string(
        dirs::home_dir()
            .context("No home dir in which to find .zuliprc found.")?
            .join(".zuliprc"),
    )?)?;
    let z_client = zulib::Client::new(zuliprc)?;

    args.command.run(z_client).await
}
