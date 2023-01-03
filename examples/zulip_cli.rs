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
    /// Get messages
    Get {
        /// Search query to narrow message search
        search_query: Vec<String>,
    },
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
    let messages = z_client
        .get_messages(GetMessagesRequest {
            anchor: Some(Anchor::MessageId(42)),
            ..GetMessagesRequest::new(1, 1)
        })
        .await?;
    Ok(())
}
