use anyhow::*;
use clap::Parser as _;
use zulip::message::*;

#[derive(clap::Parser)]
#[command(author, version, about)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    Get(GetMessagesRequest),
    #[clap(subcommand)]
    Send(SendMessageRequest),
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    flexi_logger::Logger::try_with_str("info, zulip=debug")
        .unwrap()
        .start()?;

    let zuliprc = zulip::ZulipRc::parse_from_str(&std::fs::read_to_string(
        dirs::home_dir()
            .context("No home dir in which to find .zuliprc found.")?
            .join(".zuliprc"),
    )?)?;
    let z_client = zulip::Client::new(zuliprc)?;

    match args.command {
        Command::Get(req) => {
            let messages = z_client.get_messages(req).await?.messages;
            for message in messages {
                println!("From: {} - {}", message.sender_full_name, message.timestamp);
                println!("Subject: {}", message.subject);
                println!("{}\n", message.content);
            }
        }
        Command::Send(req) => {
            println!("Sending: {req:?}");
            //z_client.send_message(req).await?;
        }
    }
    Ok(())
}
