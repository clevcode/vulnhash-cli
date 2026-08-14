use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::io::Read;

use crate::{
    client::{Client, HashResponse},
    config, output, store,
};

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
        std::io::stdin()
            .read_to_end(&mut buf)
            .map_err(|e| format!("read stdin: {e}"))?;
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

        match outcome {
            Ok(resp) => {
                let receipt_result = retain_server_receipt(&hash, &proof, &resp);
                if json {
                    let saved = store::get_entry(&hash)?;
                    let parent_id = store::get_entry(&saved.parent).ok().map(|p| p.local_id);
                    let mut value = output::entry(&saved, parent_id);
                    value["saved"] = serde_json::json!(true);
                    value["server_accepted"] = serde_json::json!(true);
                    value["receipt_saved"] = serde_json::json!(receipt_result.is_ok());
                    value["receipt_error"] = receipt_result
                        .as_ref()
                        .err()
                        .map(|e| serde_json::json!(e))
                        .unwrap_or(serde_json::Value::Null);
                    value["server_error"] = serde_json::Value::Null;
                    output::print(&value);
                    return Ok(());
                }

                println!("ID:   {}", entry.local_id);
                println!("Hash: {hash}");
                if let Some(parent) = &parent_hash {
                    println!("Follows up on: {parent}");
                }
                match receipt_result {
                    Ok(()) => {
                        let saved = store::get_entry(&hash)?;
                        println!("Certified hash: {}", saved.certified_hash);
                        println!("Server recorded: {}", resp.submitted_at);
                        println!("Receipt: {}", store::receipt_path(&hash).display());
                        println!("Saved locally and submitted to server.");
                    }
                    Err(e) => {
                        eprintln!("warning: server accepted the submission, but its private receipt was not safely retained: {e}");
                        eprintln!("warning: this submission remains marked unsynced until the receipt is recovered");
                        eprintln!("warning: recover it with 'vulnhash receipt {} --refresh --output receipt.json'", entry.local_id);
                        println!("Server recorded: {}", resp.submitted_at);
                        println!("Receipt recovery required for ID {}.", entry.local_id);
                    }
                }
            }
            Err(e) => {
                store::update_entry(&hash, |entry| entry.synced = false)?;
                if json {
                    let saved = store::get_entry(&hash)?;
                    let parent_id = store::get_entry(&saved.parent).ok().map(|p| p.local_id);
                    let mut value = output::entry(&saved, parent_id);
                    value["saved"] = serde_json::json!(true);
                    value["server_accepted"] = serde_json::json!(false);
                    value["receipt_saved"] = serde_json::json!(false);
                    value["receipt_error"] = serde_json::Value::Null;
                    value["server_error"] = serde_json::json!(e);
                    output::print(&value);
                    return Ok(());
                }
                eprintln!("warning: server submission failed: {e}");
                println!("ID:   {} (saved locally only)", entry.local_id);
                println!("Hash: {hash}");
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::receipt::{
        CERTIFIED_HASH_FORMAT, CERTIFIED_HASH_VERSION, DIGEST_ALGORITHM, SCHEMA, SubmissionReceipt,
        certified_hash,
    };
    use chrono::{SecondsFormat, TimeZone};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn local_report(content: &[u8]) -> (String, Vec<u8>) {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "vulnhash-submit-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&root).unwrap();
        config::set_dir_for_test(root);
        let hash = hex::encode(Sha256::digest(content));
        store::save(&hash, content, "").unwrap();
        (hash, content.to_vec())
    }

    fn response(hash: &str, include_receipt: bool) -> HashResponse {
        let submission_id = "11111111-1111-4111-8111-111111111111";
        let user_id = "22222222-2222-4222-8222-222222222222";
        let submitted_at = Utc.with_ymd_and_hms(2026, 8, 14, 12, 34, 56).unwrap();
        let submitted_at_unix = submitted_at.timestamp();
        let username = "researcher";
        let certified = certified_hash(submission_id, hash, submitted_at_unix, user_id, username);
        let receipt = SubmissionReceipt {
            schema: SCHEMA.into(),
            certified_hash_version: CERTIFIED_HASH_VERSION.into(),
            certified_hash_format: CERTIFIED_HASH_FORMAT.into(),
            submission_id: submission_id.into(),
            digest_algorithm: DIGEST_ALGORITHM.into(),
            document_hash: hash.into(),
            submitted_at: submitted_at.to_rfc3339_opts(SecondsFormat::Secs, true),
            submitted_at_unix,
            user_id: user_id.into(),
            username: username.into(),
            certified_hash: certified.clone(),
            extra: Default::default(),
        };
        HashResponse {
            id: Some(submission_id.into()),
            user_id: Some(user_id.into()),
            hash: hash.into(),
            algorithm: Some("sha256".into()),
            published: false,
            submitted_at: receipt.submitted_at.clone(),
            certified_hash: Some(certified),
            receipt: include_receipt.then_some(receipt),
        }
    }

    #[test]
    fn accepted_submission_is_synced_only_after_receipt_is_saved() {
        let (hash, proof) = local_report(b"report");
        let response = response(&hash, true);
        retain_server_receipt(&hash, &proof, &response).unwrap();

        let entry = store::get_entry(&hash).unwrap();
        assert!(entry.synced);
        assert_eq!(entry.certified_hash, response.certified_hash.unwrap());
        assert_eq!(store::get_receipt(&hash).unwrap().document_hash, hash);
    }

    #[test]
    fn older_response_without_a_receipt_remains_recoverable_and_unsynced() {
        let (hash, proof) = local_report(b"older server");
        let response = response(&hash, false);
        let certified = response.certified_hash.clone().unwrap();
        let error = retain_server_receipt(&hash, &proof, &response).unwrap_err();
        assert!(error.contains("omitted"), "{error}");

        let entry = store::get_entry(&hash).unwrap();
        assert!(!entry.synced);
        assert_eq!(entry.certified_hash, certified);
        assert!(entry.server_submitted_at.is_some());
    }

    #[test]
    fn modified_receipt_is_not_saved_or_marked_synced() {
        let (hash, proof) = local_report(b"modified receipt");
        let mut response = response(&hash, true);
        response.receipt.as_mut().unwrap().username = "mallory".into();
        assert!(retain_server_receipt(&hash, &proof, &response).is_err());
        assert!(!store::get_entry(&hash).unwrap().synced);
        assert!(!store::receipt_path(&hash).exists());
    }
}

/// Record an accepted response as pending, then mark it synced only after its
/// private receipt has been checked against the exact local report and synced
/// to disk.
fn retain_server_receipt(hash: &str, proof: &[u8], response: &HashResponse) -> Result<(), String> {
    if response.hash != hash {
        return Err(format!(
            "server response describes document hash {}, expected {hash}",
            response.hash
        ));
    }
    if let Some(algorithm) = &response.algorithm {
        if algorithm != "sha256" {
            return Err(format!(
                "server response uses unsupported algorithm {algorithm:?}"
            ));
        }
    }

    let server_time = DateTime::parse_from_rfc3339(&response.submitted_at)
        .map_err(|e| format!("invalid server submission timestamp: {e}"))?
        .with_timezone(&Utc);
    let reported_certified_hash = response
        .certified_hash
        .as_deref()
        .or_else(|| response.receipt.as_ref().map(|r| r.certified_hash.as_str()))
        .unwrap_or("")
        .to_string();
    if !reported_certified_hash.is_empty() {
        crate::receipt::validate_sha256(&reported_certified_hash)
            .map_err(|e| format!("invalid certified hash in server response: {e}"))?;
    }
    store::update_entry(hash, |entry| {
        entry.server_submitted_at = Some(server_time);
        entry.certified_hash = reported_certified_hash;
        entry.synced = false;
    })?;

    let receipt = response.receipt.as_ref().ok_or_else(|| {
        "server response omitted the portable receipt; update the server or re-fetch it as the owner"
            .to_string()
    })?;
    receipt.validate_report(proof)?;
    if receipt.document_hash != response.hash {
        return Err("receipt document hash differs from the server response".into());
    }
    if receipt.submitted_at != response.submitted_at {
        return Err("receipt timestamp differs from the server response".into());
    }
    if let Some(id) = &response.id {
        if id != &receipt.submission_id {
            return Err("receipt submission id differs from the server response".into());
        }
    }
    if let Some(user_id) = &response.user_id {
        if user_id != &receipt.user_id {
            return Err("receipt user id differs from the server response".into());
        }
    }
    if let Some(certified_hash) = &response.certified_hash {
        if certified_hash != &receipt.certified_hash {
            return Err("receipt certified hash differs from the server response".into());
        }
    }

    store::save_receipt(hash, receipt)?;
    store::update_entry(hash, |entry| {
        entry.server_submitted_at = Some(server_time);
        entry.certified_hash = receipt.certified_hash.clone();
        entry.synced = true;
    })
}
