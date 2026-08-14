use crate::{base, client::Client, config, manifest, output, store};

pub fn run(
    reference: &str,
    manifest_file: Option<&str>,
    offline: bool,
    rpc_override: Option<&str>,
    tor: Option<bool>,
    json: bool,
) -> Result<(), String> {
    let hash = store::resolve(reference)?;

    // A retained receipt changes the question from "does the service say this
    // hash exists?" to independently validating the portable evidence.
    if manifest_file.is_some() || offline || store::receipt_path(&hash).exists() {
        return verify_evidence(&hash, manifest_file, offline, rpc_override, tor, json);
    }

    if offline {
        return Err("offline verification requires a local report and receipt".into());
    }
    lookup_server(&hash, tor, json)
}

fn verify_evidence(
    hash: &str,
    manifest_file: Option<&str>,
    offline: bool,
    rpc_override: Option<&str>,
    tor: Option<bool>,
    json: bool,
) -> Result<(), String> {
    let receipt = store::get_receipt(hash)?;
    let (manifest_bytes, source) =
        load_manifest(hash, &receipt.certified_hash, manifest_file, offline, tor)?;
    let verified = manifest::parse(&manifest_bytes, &receipt.certified_hash)?;
    let batch = &verified.manifest;
    if receipt.submitted_at_unix >= 0
        && batch.chain_anchor.anchored_at_unix < receipt.submitted_at_unix as u64
    {
        return Err("on-chain anchor time precedes the receipt's server timestamp".into());
    }
    let chain = if offline {
        None
    } else {
        Some(verify_chain(batch, rpc_override, tor)?)
    };

    if json {
        output::print(&serde_json::json!({
            "hash": receipt.document_hash,
            "certified_hash": receipt.certified_hash,
            "username": receipt.username,
            "server_submitted_at": receipt.submitted_at,
            "receipt_verified": true,
            "manifest_verified": true,
            "leaf_index": verified.leaf_index,
            "leaf_count": batch.leaf_count,
            "merkle_root": batch.merkle_root,
            "manifest_source": source,
            "base": {
                "network": batch.chain_anchor.network,
                "chain_id": batch.chain_anchor.chain_id,
                "contract": batch.chain_anchor.contract,
                "transaction_hash": batch.chain_anchor.transaction_hash,
                "block_number": batch.chain_anchor.block_number,
                "anchored_at_unix": batch.chain_anchor.anchored_at_unix,
                "live_chain_queried": chain.is_some(),
                "live_chain_verified": chain.is_some(),
                "latest_block": chain.as_ref().map(|verified| verified.latest_block),
                "confirmations": chain.as_ref().map(|verified| verified.confirmations),
            },
            "offline": offline,
            "status": if offline { "offline-evidence-verified" } else { "independently-verified" },
        }));
        return Ok(());
    }

    println!("Document hash:    {}", receipt.document_hash);
    println!("Certified hash:   {}", receipt.certified_hash);
    println!("Researcher:       {}", receipt.username);
    println!("Server recorded:  {}", receipt.submitted_at);
    println!("Receipt:          VERIFIED against the exact local report");
    println!(
        "Batch inclusion:  VERIFIED (leaf {} of {})",
        verified.leaf_index + 1,
        batch.leaf_count
    );
    println!("Merkle root:      {}", batch.merkle_root);
    println!("Manifest source:  {source}");
    println!("Base transaction: {}", batch.chain_anchor.transaction_hash);
    println!(
        "Anchor timestamp: {} (Unix)",
        batch.chain_anchor.anchored_at_unix
    );
    if let Some(chain) = chain {
        println!(
            "Chain state:      VERIFIED at block {} ({} confirmations)",
            chain.latest_block, chain.confirmations
        );
        println!("Result: INDEPENDENTLY VERIFIED against Base mainnet.");
    } else {
        println!("Chain state:      NOT QUERIED");
        println!("Result: OFFLINE EVIDENCE VERIFIED; current Base state was not queried.");
    }
    Ok(())
}

fn verify_chain(
    manifest: &manifest::BatchManifest,
    rpc_override: Option<&str>,
    tor: Option<bool>,
) -> Result<base::ChainVerification, String> {
    let cfg = config::load();
    let rpc_url = rpc_override.unwrap_or(&cfg.base_rpc);
    if rpc_url.is_empty() {
        return Err(
            "no Base RPC is configured; set one with 'vulnhash config base-rpc URL'".into(),
        );
    }
    let runtime = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    runtime.block_on(async {
        let client = Client::connect(&cfg, tor, false).await?;
        base::verify(&client, rpc_url, manifest).await
    })
}

fn load_manifest(
    hash: &str,
    certified_hash: &str,
    manifest_file: Option<&str>,
    offline: bool,
    tor: Option<bool>,
) -> Result<(Vec<u8>, String), String> {
    if let Some(path) = manifest_file {
        let bytes = std::fs::read(path).map_err(|e| format!("read batch manifest {path}: {e}"))?;
        return Ok((bytes, path.to_string()));
    }

    let cached_path = store::manifest_path(hash);
    if cached_path.exists() {
        return Ok((
            store::manifest_bytes(hash)?,
            cached_path.display().to_string(),
        ));
    }
    if offline {
        return Err(format!(
            "offline verification needs --manifest <file> or cached public material at {}",
            cached_path.display()
        ));
    }

    let cfg = config::load();
    let runtime = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    let (bytes, raw_url) = runtime.block_on(async {
        let client = Client::connect(&cfg, tor, false).await?;
        let proof = client.anchor_proof(certified_hash).await?;
        if proof.certified_hash != certified_hash {
            return Err("anchor proof response describes a different certified hash".into());
        }
        let locator = proof.batch_manifest.ok_or_else(|| {
            format!(
                "batch manifest is not available ({:?})",
                proof.evidence_state
            )
        })?;
        if locator.publication_state != "verified" {
            return Err(format!(
                "batch manifest publication is not verified (state {})",
                locator.publication_state
            ));
        }
        let raw_url = locator.raw_url.ok_or_else(|| {
            "verified manifest response omitted its immutable raw URL".to_string()
        })?;
        let commit = locator
            .commit_sha
            .ok_or_else(|| "manifest locator omitted its exact commit SHA".to_string())?;
        let path = locator
            .path
            .ok_or_else(|| "manifest locator omitted its repository path".to_string())?;
        let repository = locator
            .repository
            .ok_or_else(|| "manifest locator omitted its repository".to_string())?;
        if !raw_url.contains(&commit) || !raw_url.contains(&path) {
            return Err("manifest raw URL is not pinned to the reported commit and path".into());
        }
        if repository.split('/').count() != 2 {
            return Err("manifest repository is not in owner/name form".into());
        }
        let body = client.public_get(&raw_url).await?;
        Ok::<_, String>((body.into_bytes(), raw_url))
    })?;

    // Validate before allowing fetched bytes into the local cache.
    manifest::parse(&bytes, certified_hash)?;
    store::save_manifest(hash, &bytes)?;
    Ok((bytes, raw_url))
}

fn lookup_server(hash: &str, tor: Option<bool>, json: bool) -> Result<(), String> {
    // No account needed. This compatibility path answers the narrower legacy
    // question when the caller has no local receipt.
    let cfg = config::load();
    let runtime = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    runtime.block_on(async {
        let client = Client::connect(&cfg, tor, false).await?;
        let resp = client.verify_hash(hash).await?;

        if json {
            output::print(&serde_json::json!({
                "hash": resp.hash,
                "certified_hash": resp.certified_hash,
                "exists": resp.exists,
                "published": resp.published,
                "submitted_at": resp.submitted_at,
                "username": resp.username,
                "independently_verified": false,
                "status": if resp.exists { "server-record-found" } else { "not-found" },
            }));
            return Ok(());
        }

        if resp.exists {
            println!("Hash:        {}", resp.hash);
            println!("Status:      FOUND IN SERVER RECORDS");
            if let Some(certified_hash) = &resp.certified_hash {
                println!("Certified:   {certified_hash}");
            }
            if let Some(timestamp) = &resp.submitted_at {
                println!("Server time: {timestamp}");
            }
            println!("Published:   {}", resp.published);
            if let Some(user) = &resp.username {
                println!("By:          {user}");
            }
            println!("Independent evidence was not checked (no local receipt).");
        } else {
            println!("Hash: {hash}");
            println!("Status: NOT FOUND");
        }
        Ok(())
    })
}
