use sha2::{Sha256, Digest};
use crate::{config, client::Client, store};

pub fn run(hash: &str, tor: bool) -> Result<(), String> {
    let proof = store::get_proof(hash)?;

    // Verify hash matches
    let computed = hex::encode(Sha256::digest(&proof));
    if computed != hash {
        return Err("hash mismatch: proof document has been modified since submission".into());
    }

    let cfg = config::load();
    if cfg.auth_token.is_empty() {
        return Err("not configured. Run 'vulnhash config' first.".into());
    }

    let content = String::from_utf8(proof).map_err(|e| format!("proof is not UTF-8: {e}"))?;

    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    rt.block_on(async {
        let client = Client::connect(&cfg, tor, false).await?;

        client.publish_hash(hash, &content).await?;
        store::update_entry(hash, |e| e.published = true)?;
        println!("Published: {hash}");
        println!("Your proof document is now publicly visible.");
        Ok(())
    })
}
