//! Machine-readable output for `--json`.
//!
//! The shapes here are deliberately not `store::Entry` and friends. The
//! on-disk format omits empty fields to stay tidy, and it is free to change
//! when the local layout does; anything parsing our output wants every key
//! present every time, and wants it to keep meaning the same thing. Keeping
//! the two apart is what lets each move without breaking the other.

use chrono::SecondsFormat;
use serde_json::{json, Value};

use crate::store::Entry;

/// One submission, as a consumer sees it.
///
/// `parent` is the parent's hash and `parent_id` its local id, both null when
/// the submission stands on its own. The id is only meaningful on this
/// machine, so a script sharing results should quote the hash.
pub fn entry(e: &Entry, parent_id: Option<u64>) -> Value {
    json!({
        "id": e.local_id,
        "hash": e.hash,
        "algorithm": e.algorithm,
        "submitted_at": e.submitted_at.to_rfc3339_opts(SecondsFormat::Secs, true),
        "published": e.published,
        "synced": e.synced,
        "parent": string_or_null(&e.parent),
        "parent_id": parent_id.map(Value::from).unwrap_or(Value::Null),
        "title": e.title,
        "description": e.description,
    })
}

/// Every entry, with each parent's local id filled in where we know it.
pub fn entries(all: &[Entry]) -> Value {
    Value::Array(
        all.iter()
            .map(|e| {
                let parent_id = all
                    .iter()
                    .find(|p| p.hash == e.parent)
                    .map(|p| p.local_id);
                entry(e, parent_id)
            })
            .collect(),
    )
}

pub fn string_or_null(s: &str) -> Value {
    if s.is_empty() {
        Value::Null
    } else {
        json!(s)
    }
}

/// Print a value as a single JSON document.
pub fn print(value: &Value) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        // Serialising a value we built ourselves cannot realistically fail,
        // but a panic here would be a poor way to find out.
        Err(e) => eprintln!("{{\"error\":\"failed to serialise output: {e}\"}}"),
    }
}

/// Report a failure in the same format as success, so a caller parsing our
/// output does not have to switch languages when something goes wrong.
pub fn print_error(message: &str) {
    let value = json!({ "error": message });
    match serde_json::to_string(&value) {
        Ok(text) => eprintln!("{text}"),
        Err(_) => eprintln!("{{\"error\":\"unprintable error\"}}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn sample(hash: &str, id: u64, parent: &str) -> Entry {
        Entry {
            hash: hash.to_string(),
            local_id: id,
            algorithm: "sha256".into(),
            submitted_at: Utc.with_ymd_and_hms(2026, 3, 13, 0, 40, 26).unwrap(),
            parent: parent.to_string(),
            title: String::new(),
            description: String::new(),
            published: false,
            synced: true,
        }
    }

    // Every key is present whether or not it holds anything, so a consumer can
    // index without checking first.
    #[test]
    fn an_entry_has_a_stable_set_of_keys() {
        let value = entry(&sample("aa", 1, ""), None);
        let object = value.as_object().unwrap();
        for key in [
            "id",
            "hash",
            "algorithm",
            "submitted_at",
            "published",
            "synced",
            "parent",
            "parent_id",
            "title",
            "description",
        ] {
            assert!(object.contains_key(key), "missing {key}");
        }
        assert_eq!(object["parent"], Value::Null);
        assert_eq!(object["parent_id"], Value::Null);
        assert_eq!(object["title"], "");
    }

    #[test]
    fn timestamps_are_rfc3339_in_utc() {
        let value = entry(&sample("aa", 1, ""), None);
        assert_eq!(value["submitted_at"], "2026-03-13T00:40:26Z");
    }

    #[test]
    fn a_parent_is_reported_by_hash_and_by_local_id() {
        let all = vec![sample("parent-hash", 1, ""), sample("child-hash", 2, "parent-hash")];
        let value = entries(&all);
        assert_eq!(value[0]["parent"], Value::Null);
        assert_eq!(value[1]["parent"], "parent-hash");
        assert_eq!(value[1]["parent_id"], 1);
    }

    // A parent submitted on another machine is not in this store, so its id is
    // unknown — but its hash still is, and that is the part that travels.
    #[test]
    fn a_parent_from_elsewhere_keeps_its_hash_without_an_id() {
        let all = vec![sample("child-hash", 1, "hash-we-do-not-have")];
        let value = entries(&all);
        assert_eq!(value[0]["parent"], "hash-we-do-not-have");
        assert_eq!(value[0]["parent_id"], Value::Null);
    }
}
