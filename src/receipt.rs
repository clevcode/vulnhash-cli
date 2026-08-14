//! Private, portable evidence issued when a submission is accepted.
//!
//! The receipt contains the exact preimage of the public certified hash. It is
//! deliberately kept beside the report and must stay private while the report
//! is sealed: the random submission id prevents offline guesses of report
//! contents.

use chrono::{DateTime, SecondsFormat, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const SCHEMA: &str = "vulnhash-submission-receipt-v1";
pub const CERTIFIED_HASH_VERSION: &str = "vulnhash-leaf-v1";
pub const CERTIFIED_HASH_FORMAT: &str = concat!(
    "sha256(0x00 || \"vulnhash-leaf-v1\\n\" || submission_id || \"\\n\" || ",
    "hash || \"\\n\" || submitted_at_unix || \"\\n\" || user_id || \"\\n\" || username)"
);
pub const DIGEST_ALGORITHM: &str = "sha256";

/// Unknown fields are retained so a newer server can add compatible metadata
/// without an older CLI discarding it when the receipt is saved or exported.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SubmissionReceipt {
    pub schema: String,
    pub certified_hash_version: String,
    pub certified_hash_format: String,
    pub submission_id: String,
    pub digest_algorithm: String,
    pub document_hash: String,
    pub submitted_at: String,
    pub submitted_at_unix: i64,
    pub user_id: String,
    pub username: String,
    pub certified_hash: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl SubmissionReceipt {
    /// Validate every v1 field and recompute the redundant certified hash.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != SCHEMA {
            return Err(format!("unsupported receipt schema {:?}", self.schema));
        }
        if self.certified_hash_version != CERTIFIED_HASH_VERSION {
            return Err(format!(
                "unsupported certified hash version {:?}",
                self.certified_hash_version
            ));
        }
        if self.certified_hash_format != CERTIFIED_HASH_FORMAT {
            return Err(format!(
                "certified hash format does not match {CERTIFIED_HASH_VERSION}"
            ));
        }
        uuid::Uuid::parse_str(&self.submission_id)
            .map_err(|e| format!("invalid submission id: {e}"))?;
        uuid::Uuid::parse_str(&self.user_id).map_err(|e| format!("invalid user id: {e}"))?;
        if self.digest_algorithm != DIGEST_ALGORITHM {
            return Err(format!(
                "unsupported digest algorithm {:?}",
                self.digest_algorithm
            ));
        }
        validate_sha256(&self.document_hash).map_err(|e| format!("invalid document hash: {e}"))?;
        validate_sha256(&self.certified_hash)
            .map_err(|e| format!("invalid certified hash: {e}"))?;
        if self.username.is_empty() {
            return Err("username is empty".into());
        }

        let submitted_at = DateTime::parse_from_rfc3339(&self.submitted_at)
            .map_err(|e| format!("invalid submission timestamp: {e}"))?;
        if submitted_at.nanosecond() != 0
            || submitted_at
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
                != self.submitted_at
        {
            return Err("submission timestamp is not canonical second-precision UTC".into());
        }
        if submitted_at.timestamp() != self.submitted_at_unix {
            return Err("submission timestamp does not match submitted_at_unix".into());
        }

        let want = certified_hash(
            &self.submission_id,
            &self.document_hash,
            self.submitted_at_unix,
            &self.user_id,
            &self.username,
        );
        if self.certified_hash != want {
            return Err(format!(
                "certified hash mismatch: got {}, recomputed {want}",
                self.certified_hash
            ));
        }
        Ok(())
    }

    /// Confirm this receipt belongs to the exact report bytes being retained.
    pub fn validate_report(&self, report: &[u8]) -> Result<(), String> {
        self.validate()?;
        let got = hex::encode(Sha256::digest(report));
        if got != self.document_hash {
            return Err(format!(
                "report hash mismatch: receipt says {}, report is {got}",
                self.document_hash
            ));
        }
        Ok(())
    }

    /// Deterministic, human-readable JSON with a trailing newline.
    pub fn to_json(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let mut data = serde_json::to_vec_pretty(self)
            .map_err(|e| format!("serialize submission receipt: {e}"))?;
        data.push(b'\n');
        Ok(data)
    }
}

pub fn parse(data: &[u8]) -> Result<SubmissionReceipt, String> {
    let receipt: SubmissionReceipt =
        serde_json::from_slice(data).map_err(|e| format!("parse submission receipt: {e}"))?;
    receipt.validate()?;
    Ok(receipt)
}

pub fn certified_hash(
    submission_id: &str,
    document_hash: &str,
    submitted_at_unix: i64,
    user_id: &str,
    username: &str,
) -> String {
    let preimage = format!(
        "{CERTIFIED_HASH_VERSION}\n{submission_id}\n{document_hash}\n{submitted_at_unix}\n{user_id}\n{username}"
    );
    let mut hash = Sha256::new();
    hash.update([0]);
    hash.update(preimage.as_bytes());
    hex::encode(hash.finalize())
}

pub fn validate_sha256(value: &str) -> Result<(), String> {
    if value.len() != 64 {
        return Err("expected 64 lowercase hexadecimal characters".into());
    }
    if value != value.to_ascii_lowercase() {
        return Err("digest is not lowercase".into());
    }
    hex::decode(value)
        .map(|_| ())
        .map_err(|e| format!("decode hexadecimal digest: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SubmissionReceipt {
        SubmissionReceipt {
            schema: SCHEMA.into(),
            certified_hash_version: CERTIFIED_HASH_VERSION.into(),
            certified_hash_format: CERTIFIED_HASH_FORMAT.into(),
            submission_id: "11111111-1111-4111-8111-111111111111".into(),
            digest_algorithm: DIGEST_ALGORITHM.into(),
            document_hash: "ab".repeat(32),
            submitted_at: "2026-08-14T12:34:56Z".into(),
            submitted_at_unix: 1_786_710_896,
            user_id: "22222222-2222-4222-8222-222222222222".into(),
            username: "researcher".into(),
            certified_hash: "8fe6c8321bd99c66290cc7720860c140489d3321d2730f111d7c1c7b18aa916f"
                .into(),
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn matches_the_server_fixture() {
        let receipt = sample();
        receipt.validate().unwrap();
        assert_eq!(
            certified_hash(
                &receipt.submission_id,
                &receipt.document_hash,
                receipt.submitted_at_unix,
                &receipt.user_id,
                &receipt.username,
            ),
            receipt.certified_hash
        );
    }

    #[test]
    fn changed_preimage_fields_are_rejected() {
        let mut receipt = sample();
        receipt.username = "someone-else".into();
        assert!(
            receipt
                .validate()
                .unwrap_err()
                .contains("certified hash mismatch")
        );

        let mut receipt = sample();
        receipt.submitted_at_unix += 1;
        assert!(
            receipt
                .validate()
                .unwrap_err()
                .contains("submitted_at_unix")
        );
    }

    #[test]
    fn timestamps_and_digests_must_be_canonical() {
        let mut receipt = sample();
        receipt.submitted_at = "2026-08-14T12:34:56+00:00".into();
        assert!(receipt.validate().is_err());

        let mut receipt = sample();
        receipt.document_hash = receipt.document_hash.to_ascii_uppercase();
        assert!(receipt.validate().is_err());
    }

    #[test]
    fn additive_fields_survive_a_round_trip() {
        let mut value = serde_json::to_value(sample()).unwrap();
        value["future_field"] = serde_json::json!({"kept": true});
        let receipt = parse(&serde_json::to_vec(&value).unwrap()).unwrap();
        let saved: Value = serde_json::from_slice(&receipt.to_json().unwrap()).unwrap();
        assert_eq!(saved["future_field"]["kept"], true);
    }

    #[test]
    fn exact_report_bytes_are_checked() {
        let mut receipt = sample();
        receipt.document_hash = hex::encode(Sha256::digest(b"report"));
        receipt.certified_hash = certified_hash(
            &receipt.submission_id,
            &receipt.document_hash,
            receipt.submitted_at_unix,
            &receipt.user_id,
            &receipt.username,
        );
        receipt.validate_report(b"report").unwrap();
        assert!(receipt.validate_report(b"changed").is_err());
    }
}
