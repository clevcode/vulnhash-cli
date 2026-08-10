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
        #[command(flatten)]
        tor: TorArgs,
        /// Use ephemeral Tor (no state persisted)
        #[arg(long)]
        tor_ephemeral: bool,
    },
    /// List submitted hashes
    #[command(alias = "ls")]
    List,
    /// Show details of a submission
    Show {
        /// Hash prefix or full hash
        hash: String,
    },
    /// Publish a finding (reveal the proof)
    Publish {
        hash: String,
        #[command(flatten)]
        tor: TorArgs,
    },
    /// Verify a hash exists on the server
    Verify {
        hash: String,
        #[command(flatten)]
        tor: TorArgs,
    },
    /// Add a note to a finding
    Note {
        hash: String,
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
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Submit { file, tor, tor_ephemeral } => {
            commands::submit::run(&file, tor.choice(), tor_ephemeral)
        }
        Commands::List => commands::list::run(),
        Commands::Show { hash } => commands::show::run(&hash),
        Commands::Publish { hash, tor } => commands::publish::run(&hash, tor.choice()),
        Commands::Verify { hash, tor } => commands::verify::run(&hash, tor.choice()),
        Commands::Note { hash, text } => commands::note::run(&hash, text.as_deref()),
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
