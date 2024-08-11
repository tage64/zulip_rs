use std::future::Future;
use std::ops::ControlFlow;
use std::pin::Pin;

use rustyline::error::ReadlineError;
use shlex::Shlex; // For splitting a string into command line arguments.

/// A result from a command.
pub type CommandResult = anyhow::Result<ControlFlow<(), ()>>;

/// Run a repl from a `clap::Subcommand`.
///
/// # Arguments
///
/// Takes a function generating a prompt, a run function for something
/// implementing `clap::Subcommand`, and a mutable reference to some data that
/// will be passed to the run-function of the command.
pub async fn run_repl<Cmds, T>(
    mut prompt: impl FnMut(&mut T) -> String,
    mut run_func: impl for<'a> FnMut(
        Cmds,
        &'a mut T,
    ) -> Pin<Box<dyn Future<Output = CommandResult> + 'a>>,
    data: &mut T,
) -> anyhow::Result<()>
where
    Cmds: clap::Subcommand + clap::FromArgMatches,
{
    // Create a super command which has all commands as subcommands. This is a so
    // called "multicall" command, (see `clap::Command::multicall` for more
    // information). The idea is that the argument list is sent to this command
    // and the first argument should be recognized as a subcommand.
    let mut super_command = clap::Command::new("")
        .multicall(true)
        .subcommand_required(true)
        .subcommand_value_name("COMMAND")
        .subcommand_help_heading("COMMANDS")
        .help_template("\n{all-args}")
        .allow_external_subcommands(true); // Needed to be able to figure out when the user has entered an invalid command.
    super_command = Cmds::augment_subcommands(super_command);

    // Initiate the Read Eval Print LOOP!
    let mut rl = rustyline::Editor::<(), rustyline::history::MemHistory>::with_history(
        rustyline::Config::builder().auto_add_history(true).build(),
        Default::default(),
    )?;
    loop {
        match rl.readline(&prompt(data)) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let mut arg_splitter = Shlex::new(line);
                // Try to parse the arguments but don't handle the result yet. Since the
                // shlex-stuff happens implace, we need to check whether that has failed first.
                let arg_matches_res = super_command.try_get_matches_from_mut(arg_splitter.by_ref());
                if arg_splitter.had_error {
                    eprintln!(
                        "Error while splitting argument list. Perhaps an unclosed quotation or \
                         unended escape."
                    );
                    continue;
                }

                let arg_matches = match arg_matches_res {
                    Ok(x) => x,
                    Err(e) => {
                        // Command line parsing failed.
                        e.print().unwrap_or_else(|f| {
                            panic!("Error: {}, Failed to print CLI parsing error: {}", f, e)
                        });
                        continue;
                    }
                };
                let args = match Cmds::from_arg_matches(&arg_matches) {
                    Ok(x) => x,
                    Err(e) if e.kind() == clap::error::ErrorKind::InvalidSubcommand => {
                        eprintln!(
                            r#"{}: command not found, try "help" for a list of all commands."#,
                            arg_matches.subcommand().unwrap().0
                        );
                        continue;
                    }
                    Err(e) => {
                        eprintln!("Failed to deserialize the command: {e}");
                        continue;
                    }
                };
                match run_func(args, data).await {
                    Ok(ControlFlow::Continue(())) => (),
                    Ok(ControlFlow::Break(())) => break,
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            Err(ReadlineError::Eof | ReadlineError::Interrupted) => break,
            Err(e) => anyhow::bail!(e),
        }
    }
    Ok(())
}
