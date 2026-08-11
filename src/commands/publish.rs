use sha2::{Sha256, Digest};
use crate::{config, client::Client, store};

/// One submission that is ready to be published.
struct Ready {
    hash: String,
    local_id: u64,
    content: String,
}

pub fn run(reference: &str, recursive: bool, tor: Option<bool>) -> Result<(), String> {
    let hash = store::resolve(reference)?;

    // Publishing is irreversible, so every document is read and checked before
    // any of them is sent. Failing halfway through a subtree would leave a
    // partial disclosure that cannot be taken back.
    let mut targets = vec![store::get_entry(&hash)?];
    if recursive {
        targets.extend(store::descendants(&hash)?);
    }

    let mut ready = Vec::new();
    let mut already = Vec::new();
    for entry in &targets {
        if entry.published {
            already.push(entry.local_id);
            continue;
        }
        let proof = store::get_proof(&entry.hash)
            .map_err(|e| format!("{} ({}): {e}", entry.local_id, short(&entry.hash)))?;
        if hex::encode(Sha256::digest(&proof)) != entry.hash {
            return Err(format!(
                "{} ({}): proof document has been modified since submission",
                entry.local_id,
                short(&entry.hash)
            ));
        }
        let content = String::from_utf8(proof)
            .map_err(|e| format!("{} ({}): proof is not UTF-8: {e}", entry.local_id, short(&entry.hash)))?;
        ready.push(Ready { hash: entry.hash.clone(), local_id: entry.local_id, content });
    }

    if ready.is_empty() {
        println!("Nothing to publish: everything selected is already published.");
        return Ok(());
    }

    let cfg = config::load();
    if cfg.auth_token.is_empty() {
        return Err("not configured. Run 'vulnhash config' first.".into());
    }

    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    rt.block_on(async {
        let client = Client::connect(&cfg, tor, false).await?;

        if recursive {
            warn_about_remote_only(&client, &hash, &targets).await;
        }

        for item in &ready {
            client.publish_hash(&item.hash, &item.content).await?;
            store::update_entry(&item.hash, |e| e.published = true)?;
            println!("Published: {} {}", item.local_id, item.hash);
        }

        if !already.is_empty() {
            let ids: Vec<String> = already.iter().map(|id| id.to_string()).collect();
            println!("Already published, left alone: {}", ids.join(", "));
        }
        match ready.len() {
            1 => println!("Your proof document is now publicly visible."),
            n => println!("{n} proof documents are now publicly visible."),
        }
        Ok(())
    })
}

/// Warn when the server knows of follow-ups this machine does not have.
///
/// Those cannot be published from here — their documents live wherever they
/// were written — and a recursive publish that silently skipped them would
/// look like it had covered the whole subtree.
async fn warn_about_remote_only(client: &Client, hash: &str, targets: &[store::Entry]) {
    let remote = match client.descendants(hash).await {
        Ok(r) => r,
        // The subtree is a courtesy check; a server that cannot answer it must
        // not stop a publish that is otherwise ready to go.
        Err(e) => {
            eprintln!("warning: could not check the server for other follow-ups: {e}");
            return;
        }
    };

    let missing: Vec<&str> = remote
        .iter()
        .filter(|r| !r.published && !targets.iter().any(|t| t.hash == r.hash))
        .map(|r| r.hash.as_str())
        .collect();
    if missing.is_empty() {
        return;
    }
    eprintln!(
        "warning: {} follow-up submission(s) exist on the server but not on this machine, \
         and are being left unpublished:",
        missing.len()
    );
    for hash in missing {
        eprintln!("  {hash}");
    }
}

fn short(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
}
