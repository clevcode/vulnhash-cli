mod base;
mod client;
mod commands;
mod config;
mod manifest;
mod output;
mod receipt;
mod store;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "vulnhash",
    about = "Cryptographic proof-of-discovery for vulnerability research"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Emit JSON instead of text, for scripts. Accepted before or after the
    /// subcommand.
    #[arg(short = 'j', long, global = true)]
    json: bool,
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
        /// Read the immutable public batch from this file instead of GitHub
        #[arg(long, value_name = "FILE")]
        manifest: Option<String>,
        /// Make no network requests; verify through the manifest root only
        #[arg(long)]
        offline: bool,
        /// Override the configured read-only Base JSON-RPC endpoint
        #[arg(long, value_name = "URL", conflicts_with = "offline")]
        rpc: Option<String>,
        #[command(flatten)]
        tor: TorArgs,
    },
    /// Export or recover the private portable receipt for a submission
    Receipt {
        /// Local id, hash prefix or full document hash
        reference: String,
        /// Write to this new file instead of stdout; "-" also means stdout
        #[arg(long, short = 'o', value_name = "FILE")]
        output: Option<String>,
        /// Re-fetch the owner-only receipt even when a local copy exists
        #[arg(long)]
        refresh: bool,
        #[command(flatten)]
        tor: TorArgs,
    },
    /// Disclose a sealed finding to one researcher, privately
    Disclose {
        /// Local id, hash prefix or full hash
        reference: String,
        /// The researcher to disclose it to (their username)
        #[arg(long, short = 't')]
        to: String,
        #[command(flatten)]
        tor: TorArgs,
    },
    /// Set the details a finding shows publicly, disclosed or not
    Details {
        /// Local id, hash prefix or full hash
        reference: String,
        /// One line saying what this is about
        #[arg(long, short = 't')]
        title: Option<String>,
        /// A longer public description
        #[arg(long, short = 'd')]
        description: Option<String>,
        #[command(flatten)]
        tor: TorArgs,
    },
    /// Add a private note. Notes stay on this machine and are never uploaded
    Note {
        /// Local id, hash prefix or full hash
        reference: String,
        /// Note text (reads from stdin if omitted)
        text: Option<String>,
    },
    /// Configure server, auth token and Tor. Run with no arguments to show
    /// the current settings.
    Config {
        /// Config key to set: server, onion-server, token, base-rpc, tor, tor-ephemeral
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

    let json = cli.json;
    let result = match cli.command {
        Commands::Submit {
            file,
            parent,
            tor,
            tor_ephemeral,
        } => commands::submit::run(&file, parent.as_deref(), tor.choice(), tor_ephemeral, json),
        Commands::List => commands::list::run(json),
        Commands::Show { reference } => commands::show::run(&reference, json),
        Commands::Publish {
            reference,
            recursive,
            tor,
        } => commands::publish::run(&reference, recursive, tor.choice(), json),
        Commands::Verify {
            reference,
            manifest,
            offline,
            rpc,
            tor,
        } => commands::verify::run(
            &reference,
            manifest.as_deref(),
            offline,
            rpc.as_deref(),
            tor.choice(),
            json,
        ),
        Commands::Receipt {
            reference,
            output,
            refresh,
            tor,
        } => commands::receipt::run(&reference, output.as_deref(), refresh, tor.choice(), json),
        Commands::Disclose { reference, to, tor } => {
            commands::disclose::run(&reference, &to, tor.choice(), json)
        }
        Commands::Details {
            reference,
            title,
            description,
            tor,
        } => commands::details::run(
            &reference,
            title.as_deref(),
            description.as_deref(),
            tor.choice(),
            json,
        ),
        Commands::Note { reference, text } => {
            commands::note::run(&reference, text.as_deref(), json)
        }
        Commands::Config { key, value } => {
            commands::config::run(key.as_deref(), value.as_deref(), json)
        }
        Commands::Server { port } => commands::server::run(port),
        Commands::Version => {
            if json {
                output::print(&serde_json::json!({ "version": env!("CARGO_PKG_VERSION") }));
            } else {
                println!("vulnhash {}", env!("CARGO_PKG_VERSION"));
            }
            Ok(())
        }
    };

    if let Err(e) = result {
        // A caller parsing our output should not have to switch formats the
        // moment something fails.
        if json {
            output::print_error(&e);
        } else {
            eprintln!("error: {e}");
        }
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
        assert_eq!(
            tor_choice(&["vulnhash", "verify", "abc", "--tor"]),
            Some(true)
        );
        assert_eq!(
            tor_choice(&["vulnhash", "verify", "abc", "--no-tor"]),
            Some(false)
        );
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
    fn disclose_requires_a_recipient() {
        // --to is mandatory: disclosing to nobody is meaningless, and a
        // missing recipient should fail parsing rather than at runtime.
        assert!(Cli::try_parse_from(["vulnhash", "disclose", "3"]).is_err());
        match Cli::parse_from(["vulnhash", "disclose", "3", "--to", "acmecorp"]).command {
            Commands::Disclose { reference, to, .. } => {
                assert_eq!(reference, "3");
                assert_eq!(to, "acmecorp");
            }
            _ => panic!("expected a disclose command"),
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
    fn receipt_can_be_refetched_and_exported() {
        match Cli::parse_from([
            "vulnhash",
            "receipt",
            "3",
            "--refresh",
            "--output",
            "receipt.json",
            "--tor",
        ])
        .command
        {
            Commands::Receipt {
                reference,
                output,
                refresh,
                tor,
            } => {
                assert_eq!(reference, "3");
                assert_eq!(output.as_deref(), Some("receipt.json"));
                assert!(refresh);
                assert_eq!(tor.choice(), Some(true));
            }
            _ => panic!("expected a receipt command"),
        }
    }

    #[test]
    fn verify_accepts_an_explicit_manifest_and_offline_mode() {
        match Cli::parse_from([
            "vulnhash",
            "verify",
            "3",
            "--manifest",
            "/tmp/batch.json",
            "--offline",
        ])
        .command
        {
            Commands::Verify {
                reference,
                manifest,
                offline,
                rpc,
                ..
            } => {
                assert_eq!(reference, "3");
                assert_eq!(manifest.as_deref(), Some("/tmp/batch.json"));
                assert!(offline);
                assert_eq!(rpc, None);
            }
            _ => panic!("expected a verify command"),
        }
    }

    // Global, so a script can put it wherever it reads best rather than
    // remembering which side of the subcommand it belongs on.
    #[test]
    fn json_is_accepted_on_either_side_of_the_subcommand() {
        for args in [
            vec!["vulnhash", "list", "--json"],
            vec!["vulnhash", "--json", "list"],
            vec!["vulnhash", "list", "-j"],
            vec!["vulnhash", "-j", "list"],
        ] {
            assert!(Cli::parse_from(&args).json, "{args:?}");
        }
        assert!(!Cli::parse_from(["vulnhash", "list"]).json);
    }

    #[test]
    fn json_reaches_every_command_that_reports_something() {
        for args in [
            vec!["vulnhash", "show", "1", "-j"],
            vec!["vulnhash", "verify", "1", "-j"],
            vec!["vulnhash", "publish", "1", "-j"],
            vec!["vulnhash", "submit", "f.md", "-j"],
            vec!["vulnhash", "note", "1", "text", "-j"],
            vec!["vulnhash", "config", "-j"],
        ] {
            assert!(Cli::parse_from(&args).json, "{args:?}");
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
