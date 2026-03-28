use sha2::{Sha256, Digest};
use std::io::Read;

use crate::{config, client::Client, store};

pub fn run(file: &str, tor: bool, tor_ephemeral: bool) -> Result<(), String> {
    // Read input
    let proof = if file == "-" {
        let mut buf = Vec::new();
        std::io::stdin().read_to_end(&mut buf).map_err(|e| format!("read stdin: {e}"))?;
        buf
    } else {
        std::fs::read(file).map_err(|e| format!("read file: {e}"))?
    };

    if proof.is_empty() {
        return Err("empty input".into());
    }

    // Hash
    let hash = hex::encode(Sha256::digest(&proof));

    // Save locally
    store::save(&hash, &proof)?;
    println!("Saved locally: ~/.vulnhash/{hash}/PROOF.md");

    // Submit to server
    let cfg = config::load();
    if cfg.auth_token.is_empty() {
        println!("Hash: {hash}");
        println!("Not submitted to server (no auth token configured).");
        println!("Run 'vulnhash config token <your-token>' to set up.");
        return Ok(());
    }

    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    rt.block_on(async {
        let client = if tor {
            Client::with_tor(&cfg, tor_ephemeral).await?
        } else {
            Client::new(&cfg)
        };

        match client.submit_hash(&hash, "sha256").await {
            Ok(resp) => {
                store::update_entry(&hash, |e| e.synced = true)?;
                println!("Hash: {hash}");
                println!("Submitted at: {}", resp.submitted_at);
                println!("Saved locally and submitted to server.");
            }
            Err(e) => {
                eprintln!("warning: server submission failed: {e}");
                println!("Hash: {hash} (saved locally only)");
            }
        }
        Ok(())
    })
}
