//! Validation of the immutable public batch record.

use chrono::{DateTime, FixedOffset, Timelike, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const SCHEMA: &str = "vulnhash-anchor-batch-v1";
pub const BASE_NETWORK: &str = "base-mainnet";
pub const BASE_CHAIN_ID: u64 = 8453;
pub const ANCHORED_EVENT: &str = "Anchored(bytes32,uint256,uint256)";

#[derive(Clone, Debug, Deserialize)]
pub struct TreeRules {
    pub certified_hash_version: String,
    pub node_hash_algorithm: String,
    pub internal_node_prefix: String,
    pub padding: String,
    pub minimum_width: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BaseAnchor {
    pub network: String,
    pub chain_id: u64,
    pub contract: String,
    pub transaction_hash: String,
    pub block_number: u64,
    pub receipt_success: bool,
    pub anchored_at_unix: u64,
    pub event: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BatchManifest {
    pub schema: String,
    pub period_start: String,
    pub period_end: String,
    pub leaf_count: usize,
    pub leaves: Vec<String>,
    pub merkle_root: String,
    pub tree: TreeRules,
    pub chain_anchor: BaseAnchor,
}

#[derive(Debug)]
pub struct VerifiedManifest {
    pub manifest: BatchManifest,
    pub leaf_index: usize,
}

pub fn parse(data: &[u8], certified_hash: &str) -> Result<VerifiedManifest, String> {
    let manifest: BatchManifest =
        serde_json::from_slice(data).map_err(|e| format!("parse batch manifest: {e}"))?;
    manifest.validate()?;
    let matches: Vec<usize> = manifest
        .leaves
        .iter()
        .enumerate()
        .filter_map(|(index, leaf)| (leaf == certified_hash).then_some(index))
        .collect();
    match matches.as_slice() {
        [index] => Ok(VerifiedManifest {
            manifest,
            leaf_index: *index,
        }),
        [] => Err(format!(
            "certified hash {certified_hash} is absent from the batch manifest"
        )),
        _ => Err(format!(
            "certified hash {certified_hash} occurs more than once in the batch manifest"
        )),
    }
}

impl BatchManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != SCHEMA {
            return Err(format!(
                "unsupported batch manifest schema {:?}",
                self.schema
            ));
        }
        let start = parse_manifest_time("period_start", &self.period_start)?;
        let end = parse_manifest_time("period_end", &self.period_end)?;
        if end < start {
            return Err("period_end precedes period_start".into());
        }
        if self.leaves.is_empty() || self.leaf_count != self.leaves.len() {
            return Err(format!(
                "leaf_count {} does not match {} leaves",
                self.leaf_count,
                self.leaves.len()
            ));
        }

        let mut seen = HashSet::with_capacity(self.leaves.len());
        for (index, leaf) in self.leaves.iter().enumerate() {
            crate::receipt::validate_sha256(leaf)
                .map_err(|_| format!("leaf {index} is not a lowercase SHA-256 digest"))?;
            if !seen.insert(leaf) {
                return Err(format!("leaf {index} duplicates an earlier certified hash"));
            }
        }

        if self.tree.certified_hash_version != crate::receipt::CERTIFIED_HASH_VERSION
            || self.tree.node_hash_algorithm != "sha256"
            || self.tree.internal_node_prefix != "01"
            || self.tree.padding != "duplicate-last-to-power-of-two-minimum-two"
            || self.tree.minimum_width != 2
        {
            return Err(format!("tree rules do not match {SCHEMA}"));
        }
        crate::receipt::validate_sha256(&self.merkle_root)
            .map_err(|e| format!("invalid Merkle root: {e}"))?;
        let rebuilt = merkle_root(&self.leaves)?;
        if rebuilt != self.merkle_root {
            return Err(format!(
                "merkle root mismatch: manifest says {}, rebuilt {rebuilt}",
                self.merkle_root
            ));
        }
        self.chain_anchor.validate()
    }
}

impl BaseAnchor {
    pub fn validate(&self) -> Result<(), String> {
        if self.network != BASE_NETWORK || self.chain_id != BASE_CHAIN_ID {
            return Err(format!(
                "chain anchor is not Base mainnet (chain {BASE_CHAIN_ID})"
            ));
        }
        validate_fixed_hex(&self.contract, 20)
            .map_err(|e| format!("invalid contract address: {e}"))?;
        validate_fixed_hex(&self.transaction_hash, 32)
            .map_err(|e| format!("invalid transaction hash: {e}"))?;
        if self.block_number == 0 {
            return Err("missing Base block number".into());
        }
        if !self.receipt_success {
            return Err("Base transaction receipt was not successful".into());
        }
        if self.anchored_at_unix == 0 {
            return Err("missing on-chain anchor timestamp".into());
        }
        if self.event != ANCHORED_EVENT {
            return Err(format!("unexpected anchor event {:?}", self.event));
        }
        Ok(())
    }
}

pub fn merkle_root(leaves: &[String]) -> Result<String, String> {
    if leaves.is_empty() {
        return Err("cannot build an empty Merkle tree".into());
    }
    let mut layer = leaves.to_vec();
    for leaf in &layer {
        crate::receipt::validate_sha256(leaf)?;
    }
    if layer.len() == 1 {
        layer.push(layer[0].clone());
    }
    while !layer.len().is_power_of_two() {
        layer.push(layer.last().unwrap().clone());
    }
    while layer.len() > 1 {
        let mut next = Vec::with_capacity(layer.len() / 2);
        for pair in layer.chunks_exact(2) {
            next.push(hash_pair(&pair[0], &pair[1])?);
        }
        layer = next;
    }
    Ok(layer.remove(0))
}

fn hash_pair(left: &str, right: &str) -> Result<String, String> {
    let left = hex::decode(left).map_err(|e| format!("decode left Merkle node: {e}"))?;
    let right = hex::decode(right).map_err(|e| format!("decode right Merkle node: {e}"))?;
    let mut hash = Sha256::new();
    hash.update([0x01]);
    hash.update(left);
    hash.update(right);
    Ok(hex::encode(hash.finalize()))
}

fn parse_manifest_time(name: &str, value: &str) -> Result<DateTime<Utc>, String> {
    let parsed: DateTime<FixedOffset> =
        DateTime::parse_from_rfc3339(value).map_err(|e| format!("invalid {name}: {e}"))?;
    let utc = parsed.with_timezone(&Utc);
    if parsed.offset().local_minus_utc() != 0 || canonical_rfc3339_nano(utc) != value {
        return Err(format!("{name} is not canonical UTC RFC3339"));
    }
    Ok(utc)
}

fn canonical_rfc3339_nano(time: DateTime<Utc>) -> String {
    let mut value = time.format("%Y-%m-%dT%H:%M:%S").to_string();
    if time.nanosecond() != 0 {
        let nanos = format!("{:09}", time.nanosecond());
        value.push('.');
        value.push_str(nanos.trim_end_matches('0'));
    }
    value.push('Z');
    value
}

fn validate_fixed_hex(value: &str, bytes: usize) -> Result<(), String> {
    if !value.starts_with("0x") {
        return Err("missing 0x prefix".into());
    }
    if value != value.to_ascii_lowercase() {
        return Err("hexadecimal value is not lowercase".into());
    }
    let decoded = hex::decode(&value[2..]).map_err(|e| format!("decode hexadecimal value: {e}"))?;
    if decoded.len() != bytes {
        return Err(format!("got {} bytes, want {bytes}", decoded.len()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> BatchManifest {
        BatchManifest {
            schema: SCHEMA.into(),
            period_start: "2026-08-14T12:00:00Z".into(),
            period_end: "2026-08-14T13:00:00.000000123Z".into(),
            leaf_count: 3,
            leaves: vec!["11".repeat(32), "22".repeat(32), "33".repeat(32)],
            merkle_root: "b830e7c9b9f8d3e70091a1e552847b7cb588c21309781df3b782ef2d26ccd18a".into(),
            tree: TreeRules {
                certified_hash_version: crate::receipt::CERTIFIED_HASH_VERSION.into(),
                node_hash_algorithm: "sha256".into(),
                internal_node_prefix: "01".into(),
                padding: "duplicate-last-to-power-of-two-minimum-two".into(),
                minimum_width: 2,
            },
            chain_anchor: BaseAnchor {
                network: BASE_NETWORK.into(),
                chain_id: BASE_CHAIN_ID,
                contract: format!("0x{}", "12".repeat(20)),
                transaction_hash: format!("0x{}", "34".repeat(32)),
                block_number: 49_840_930,
                receipt_success: true,
                anchored_at_unix: 1_765_800_000,
                event: ANCHORED_EVENT.into(),
            },
        }
    }

    #[test]
    fn matches_the_server_padding_and_root_fixture() {
        let manifest = sample();
        manifest.validate().unwrap();
        assert_eq!(merkle_root(&manifest.leaves).unwrap(), manifest.merkle_root);
    }

    #[test]
    fn changed_order_count_root_and_rules_are_rejected() {
        let mut changed = sample();
        changed.leaves.swap(0, 1);
        assert!(changed.validate().is_err());

        let mut changed = sample();
        changed.leaf_count += 1;
        assert!(changed.validate().is_err());

        let mut changed = sample();
        changed.merkle_root = "00".repeat(32);
        assert!(changed.validate().is_err());

        let mut changed = sample();
        changed.tree.internal_node_prefix = "00".into();
        assert!(changed.validate().is_err());
    }

    #[test]
    fn duplicate_or_absent_target_leaf_is_rejected() {
        let mut duplicate = sample();
        duplicate.leaves[1] = duplicate.leaves[0].clone();
        duplicate.merkle_root = merkle_root(&duplicate.leaves).unwrap();
        let data = serde_json::to_vec(&serde_json::json!({
            "schema": duplicate.schema,
            "period_start": duplicate.period_start,
            "period_end": duplicate.period_end,
            "leaf_count": duplicate.leaf_count,
            "leaves": duplicate.leaves,
            "merkle_root": duplicate.merkle_root,
            "tree": {
                "certified_hash_version": duplicate.tree.certified_hash_version,
                "node_hash_algorithm": duplicate.tree.node_hash_algorithm,
                "internal_node_prefix": duplicate.tree.internal_node_prefix,
                "padding": duplicate.tree.padding,
                "minimum_width": duplicate.tree.minimum_width,
            },
            "chain_anchor": {
                "network": duplicate.chain_anchor.network,
                "chain_id": duplicate.chain_anchor.chain_id,
                "contract": duplicate.chain_anchor.contract,
                "transaction_hash": duplicate.chain_anchor.transaction_hash,
                "block_number": duplicate.chain_anchor.block_number,
                "receipt_success": duplicate.chain_anchor.receipt_success,
                "anchored_at_unix": duplicate.chain_anchor.anchored_at_unix,
                "event": duplicate.chain_anchor.event,
            }
        }))
        .unwrap();
        assert!(parse(&data, &"11".repeat(32)).is_err());

        let valid = serde_json::to_vec(&serde_json::json!({
            "schema": sample().schema,
            "period_start": sample().period_start,
            "period_end": sample().period_end,
            "leaf_count": sample().leaf_count,
            "leaves": sample().leaves,
            "merkle_root": sample().merkle_root,
            "tree": {
                "certified_hash_version": sample().tree.certified_hash_version,
                "node_hash_algorithm": sample().tree.node_hash_algorithm,
                "internal_node_prefix": sample().tree.internal_node_prefix,
                "padding": sample().tree.padding,
                "minimum_width": sample().tree.minimum_width,
            },
            "chain_anchor": {
                "network": sample().chain_anchor.network,
                "chain_id": sample().chain_anchor.chain_id,
                "contract": sample().chain_anchor.contract,
                "transaction_hash": sample().chain_anchor.transaction_hash,
                "block_number": sample().chain_anchor.block_number,
                "receipt_success": sample().chain_anchor.receipt_success,
                "anchored_at_unix": sample().chain_anchor.anchored_at_unix,
                "event": sample().chain_anchor.event,
            }
        }))
        .unwrap();
        assert!(parse(&valid, &"44".repeat(32)).is_err());
    }

    #[test]
    fn canonical_go_rfc3339_nano_forms_are_accepted() {
        for timestamp in [
            "2026-08-14T13:00:00Z",
            "2026-08-14T13:00:00.1Z",
            "2026-08-14T13:00:00.1234Z",
            "2026-08-14T13:00:00.123456789Z",
        ] {
            assert!(
                parse_manifest_time("time", timestamp).is_ok(),
                "{timestamp}"
            );
        }
        assert!(parse_manifest_time("time", "2026-08-14T13:00:00+00:00").is_err());
        assert!(parse_manifest_time("time", "2026-08-14T13:00:00.100Z").is_err());
    }
}
