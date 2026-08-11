mod config;
mod store;
mod client;
mod commands;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "vulnhash", about = "Cryptographic proof-of-discovery for vulnerability research")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Per-command override of the configured Tor setting.
///
/// The two flags override each other rather than conflicting, so the last one
/// on the command line wins. That keeps a wrapper script able to append
/// --no-tor without having to know what it is overriding.
#[derive(Args, Clone, Copy)]
struct TorArgs {
    /// Route through Tor, whatever the config says
    #[arg(long, overrides_with = "no_tor")]
    tor: bool,
    /// Connect directly, whatever the config says
    #[arg(long = "no-tor", overrides_with = "tor")]
    no_tor: bool,
}

impl TorArgs {
    /// None means "no override": fall back to the configured setting.
    fn choice(self) -> Option<bool> {
        match (self.tor, self.no_tor) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Hash and submit a proof document
    Submit {
        /// File path, or "-" for stdin
        file: String,
        /// Record this as a follow-up to an earlier submission, named by
        /// local id, hash prefix or full hash
        #[arg(long, short = 'p', value_name = "REF")]
        parent: Option<String>,
        #[command(flatten)]
        tor: TorArgs,
        /// Use ephemeral Tor (no state persisted)
        #[arg(long)]
        tor_ephemeral: bool,
    },
    /// List submitted hashes with their local ids
    #[command(alias = "ls")]
    List,
    /// Show details of a submission
    Show {
        /// Local id, hash prefix or full hash
        reference: String,
    },
    /// Publish a finding (reveal the proof)
    Publish {
        /// Local id, hash prefix or full hash
        reference: String,
        /// Also publish every follow-up submission below this one
        #[arg(long, short = 'R')]
        recursive: bool,
        #[command(flatten)]
        tor: TorArgs,
    },
    /// Verify a hash exists on the server
    Verify {
        /// Local id, hash prefix or full hash
        reference: String,
        #[command(flatten)]
        tor: TorArgs,
    },
    /// Add a note to a finding
    Note {
        /// Local id, hash prefix or full hash
        reference: String,
        /// Note text (reads from stdin if omitted)
        text: Option<String>,
    },
    /// Configure server, auth token and Tor. Run with no arguments to show
    /// the current settings.
    Config {
        /// Config key to set: server, token, tor, tor-ephemeral
        key: Option<String>,
        /// Value to set
        value: Option<String>,
    },
    /// Start local web UI
    Server {
        #[arg(long, short, default_value_t = 9090)]
        port: u16,
    },
    /// Show version
    Version,
}

fn main() {
    restore_sigpipe();
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Submit { file, parent, tor, tor_ephemeral } => {
            commands::submit::run(&file, parent.as_deref(), tor.choice(), tor_ephemeral)
        }
        Commands::List => commands::list::run(),
        Commands::Show { reference } => commands::show::run(&reference),
        Commands::Publish { reference, recursive, tor } => {
            commands::publish::run(&reference, recursive, tor.choice())
        }
        Commands::Verify { reference, tor } => commands::verify::run(&reference, tor.choice()),
        Commands::Note { reference, text } => commands::note::run(&reference, text.as_deref()),
        Commands::Config { key, value } => commands::config::run(key.as_deref(), value.as_deref()),
        Commands::Server { port } => commands::server::run(port),
        Commands::Version => {
            println!("vulnhash {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// Die quietly when a pipe closes, the way every other command does.
///
/// Rust ignores SIGPIPE at startup, so a closed pipe surfaces as a write
/// error and `println!` panics on it: `vulnhash show 2 | head -4` printed a
/// panic and exited 101 instead of simply stopping.
#[cfg(unix)]
fn restore_sigpipe() {
    // Safe in the sense that matters here: this runs before any thread is
    // started, and it only restores the disposition the OS gives us by
    // default.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_sigpipe() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn tor_choice(args: &[&str]) -> Option<bool> {
        match Cli::parse_from(args).command {
            Commands::Verify { tor, .. } => tor.choice(),
            _ => panic!("expected a verify command"),
        }
    }

    #[test]
    fn no_flag_defers_to_the_config() {
        assert_eq!(tor_choice(&["vulnhash", "verify", "abc"]), None);
    }

    #[test]
    fn each_flag_forces_its_direction() {
        assert_eq!(tor_choice(&["vulnhash", "verify", "abc", "--tor"]), Some(true));
        assert_eq!(tor_choice(&["vulnhash", "verify", "abc", "--no-tor"]), Some(false));
    }

    // Overriding rather than conflicting lets a wrapper append --no-tor without
    // knowing whether an earlier --tor is present.
    #[test]
    fn the_last_flag_wins() {
        assert_eq!(
            tor_choice(&["vulnhash", "verify", "abc", "--tor", "--no-tor"]),
            Some(false)
        );
        assert_eq!(
            tor_choice(&["vulnhash", "verify", "abc", "--no-tor", "--tor"]),
            Some(true)
        );
    }

    // Publishing a subtree has to be asked for explicitly, since it discloses
    // more than the submission the user named.
    #[test]
    fn recursive_publish_is_opt_in() {
        for (args, want) in [
            (vec!["vulnhash", "publish", "3"], false),
            (vec!["vulnhash", "publish", "3", "-R"], true),
            (vec!["vulnhash", "publish", "3", "--recursive"], true),
        ] {
            match Cli::parse_from(&args).command {
                Commands::Publish { recursive, .. } => assert_eq!(recursive, want, "{args:?}"),
                _ => panic!("expected a publish command"),
            }
        }
    }

    #[test]
    fn submit_takes_a_parent_either_way() {
        for args in [
            vec!["vulnhash", "submit", "f.md", "--parent", "3"],
            vec!["vulnhash", "submit", "f.md", "-p", "3"],
        ] {
            match Cli::parse_from(&args).command {
                Commands::Submit { parent, .. } => assert_eq!(parent.as_deref(), Some("3")),
                _ => panic!("expected a submit command"),
            }
        }

        match Cli::parse_from(["vulnhash", "submit", "f.md"]).command {
            Commands::Submit { parent, .. } => assert_eq!(parent, None),
            _ => panic!("expected a submit command"),
        }
    }

    #[test]
    fn submit_and_publish_take_the_same_flags() {
        for args in [
            vec!["vulnhash", "submit", "f.md", "--no-tor"],
            vec!["vulnhash", "publish", "abc", "--no-tor"],
        ] {
            let choice = match Cli::parse_from(&args).command {
                Commands::Submit { tor, .. } | Commands::Publish { tor, .. } => tor.choice(),
                _ => panic!("unexpected command"),
            };
            assert_eq!(choice, Some(false), "{args:?}");
        }
    }
}
