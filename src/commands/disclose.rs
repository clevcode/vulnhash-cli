use sha2::{Digest, Sha256};

use crate::{client::Client, config, output, store};

/// Show one researcher a sealed finding, privately, without publishing it.
///
/// This is how a dispute is settled: not by argument, but by letting the other
/// party read what you found. The report is uploaded with the grant and
/// verified against the registered hash, so the recipient sees exactly what
/// was timestamped — and only that recipient. It does not make the finding
/// public, and it cannot be truly undone: revoking stops further access, but
/// what has been read has been read.
pub fn run(
    reference: &str,
    to: &str,
    tor: Option<bool>,
    json: bool,
) -> Result<(), String> {
    let hash = store::resolve(reference)?;
    let entry = store::get_entry(&hash)?;

    // Send the report the same way publish does: read it locally, confirm it
    // still hashes to what was registered, and let the server check again.
    let proof = store::get_proof(&hash)?;
    if hex::encode(Sha256::digest(&proof)) != hash {
        return Err("proof document has been modified since submission".into());
    }
    let content = String::from_utf8(proof).map_err(|e| format!("proof is not UTF-8: {e}"))?;

    let cfg = config::load();
    if cfg.auth_token.is_empty() {
        return Err("not configured. Run 'vulnhash config' first.".into());
    }

    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    rt.block_on(async {
        let client = Client::connect(&cfg, tor, false).await?;
        client.disclose(&hash, to, &content).await?;

        if json {
            output::print(&serde_json::json!({
                "status": "disclosed",
                "id": entry.local_id,
                "hash": hash,
                "to": to,
            }));
        } else {
            println!("Disclosed {} {} to {to}.", entry.local_id, hash);
            println!("They can now read the full report. The finding is not public.");
        }
        Ok(())
    })
}
