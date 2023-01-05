use anyhow::*;
use clap::Parser as _;
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
    Messages(GetMessagesRequest),
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
            Ls::Messages(req) => {
                let mut messages = z_client.get_messages(req).await?.messages;
                messages.sort_by_key(|x| x.id);
                for message in messages {
                    println!("From: {} - {}", message.sender_full_name, message.timestamp);
                    println!("Subject: {}", message.subject);
                    println!("{}\n", message.content);
                }
            }
            Ls::Streams(req) => {
                let streams = z_client.get_streams(&req).await?;
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
