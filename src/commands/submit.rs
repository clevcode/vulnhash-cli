use sha2::{Sha256, Digest};
use std::io::Read;

use crate::{config, client::Client, output, store};

pub fn run(
    file: &str,
    parent: Option<&str>,
    tor: Option<bool>,
    tor_ephemeral: bool,
    json: bool,
) -> Result<(), String> {
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

    // A parent named by local id or prefix has to be resolved here; a full
    // hash passes through untouched, so a submission made from another
    // machine can still be followed up on.
    let parent_hash = match parent {
        Some(reference) => Some(store::resolve(reference)?),
        None => None,
    };

    // Hash
    let hash = hex::encode(Sha256::digest(&proof));
    if Some(&hash) == parent_hash.as_ref() {
        return Err("a submission cannot follow up on itself".into());
    }

    // Save locally
    let entry = store::save(&hash, &proof, parent_hash.as_deref().unwrap_or(""))?;
    if !json {
        println!("Saved locally: ~/.vulnhash/{hash}/PROOF.md");
    }

    // Submit to server
    let cfg = config::load();
    if cfg.auth_token.is_empty() {
        if json {
            let parent_id = store::get_entry(&entry.parent).ok().map(|p| p.local_id);
            let mut value = output::entry(&entry, parent_id);
            value["saved"] = serde_json::json!(true);
            value["server_error"] = serde_json::json!("no auth token configured");
            output::print(&value);
            return Ok(());
        }
        println!("ID:   {}", entry.local_id);
        println!("Hash: {hash}");
        println!("Not submitted to server (no auth token configured).");
        println!("Run 'vulnhash config token <your-token>' to set up.");
        return Ok(());
    }

    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    rt.block_on(async {
        let client = Client::connect(&cfg, tor, tor_ephemeral).await?;

        let outcome = client.submit_hash(&hash, "sha256", parent_hash.as_deref()).await;

        if json {
            store::update_entry(&hash, |e| e.synced = outcome.is_ok())?;
            let saved = store::get_entry(&hash)?;
            let parent_id = store::get_entry(&saved.parent).ok().map(|p| p.local_id);
            let mut value = output::entry(&saved, parent_id);
            value["saved"] = serde_json::json!(true);
            value["server_error"] = match &outcome {
                Ok(_) => serde_json::Value::Null,
                Err(e) => serde_json::json!(e),
            };
            output::print(&value);
            return Ok(());
        }

        match outcome {
            Ok(resp) => {
                store::update_entry(&hash, |e| e.synced = true)?;
                println!("ID:   {}", entry.local_id);
                println!("Hash: {hash}");
                if let Some(parent) = &parent_hash {
                    println!("Follows up on: {parent}");
                }
                println!("Submitted at: {}", resp.submitted_at);
                println!("Saved locally and submitted to server.");
            }
            Err(e) => {
                eprintln!("warning: server submission failed: {e}");
                println!("ID:   {} (saved locally only)", entry.local_id);
                println!("Hash: {hash}");
            }
        }
        Ok(())
    })
}
