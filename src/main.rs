mod config;
mod store;
mod client;
mod commands;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "vulnhash", about = "Cryptographic proof-of-discovery for vulnerability research")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Hash and submit a proof document
    Submit {
        /// File path, or "-" for stdin
        file: String,
        /// Route through Tor
        #[arg(long)]
        tor: bool,
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
        #[arg(long)]
        tor: bool,
    },
    /// Verify a hash exists on the server
    Verify {
        hash: String,
        #[arg(long)]
        tor: bool,
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
        Commands::Submit { file, tor, tor_ephemeral } => commands::submit::run(&file, tor, tor_ephemeral),
        Commands::List => commands::list::run(),
        Commands::Show { hash } => commands::show::run(&hash),
        Commands::Publish { hash, tor } => commands::publish::run(&hash, tor),
        Commands::Verify { hash, tor } => commands::verify::run(&hash, tor),
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
