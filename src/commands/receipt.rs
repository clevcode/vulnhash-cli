use std::io::Write;
use std::path::Path;

use crate::{client::Client, config, receipt, store};

pub fn run(
    reference: &str,
    output: Option<&str>,
    refresh: bool,
    tor: Option<bool>,
    json: bool,
) -> Result<(), String> {
    let hash = store::resolve(reference)?;

    if refresh || store::receipt_bytes(&hash).is_err() {
        recover(&hash, tor)?;
    }
    let data = store::receipt_bytes(&hash)?;

    match output {
        Some("-") | None => {
            std::io::stdout()
                .write_all(&data)
                .map_err(|e| format!("write receipt to stdout: {e}"))?;
        }
        Some(path) => {
            store::export_receipt(Path::new(path), &data)?;
            if json {
                crate::output::print(&serde_json::json!({
                    "hash": hash,
                    "output": path,
                    "written": true,
                }));
            } else {
                println!("Wrote private receipt to {path}");
            }
        }
    }
    Ok(())
}

fn recover(hash: &str, tor: Option<bool>) -> Result<(), String> {
    let entry = store::get_entry(hash)?;
    if entry.certified_hash.is_empty() {
        return Err(
            "no certified hash is stored for this submission, so its receipt cannot be re-fetched"
                .into(),
        );
    }
    let cfg = config::load();
    if cfg.auth_token.is_empty() {
        return Err("an auth token is required to re-fetch this private receipt".into());
    }

    let runtime = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    let raw = runtime.block_on(async {
        let client = Client::connect(&cfg, tor, false).await?;
        client.receipt(&entry.certified_hash).await
    })?;
    let submission_receipt = receipt::parse(raw.as_bytes())?;
    if submission_receipt.certified_hash != entry.certified_hash {
        return Err("re-fetched receipt does not match the requested certified hash".into());
    }
    store::save_receipt(hash, &submission_receipt)?;
    let server_time = chrono::DateTime::parse_from_rfc3339(&submission_receipt.submitted_at)
        .map_err(|e| format!("invalid receipt submission timestamp: {e}"))?
        .with_timezone(&chrono::Utc);
    store::update_entry(hash, |saved| {
        saved.certified_hash = submission_receipt.certified_hash.clone();
        saved.server_submitted_at = Some(server_time);
        saved.synced = true;
    })
}
