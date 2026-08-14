use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{
    config,
    receipt::{self, SubmissionReceipt},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub hash: String,
    /// Short local handle, usable anywhere a hash is. Assigned once and never
    /// reused, so a number that named a submission yesterday cannot name a
    /// different one today.
    #[serde(default)]
    pub local_id: u64,
    pub algorithm: String,
    /// When these bytes were saved locally. This predates server receipts and
    /// remains separate from the server-recorded time committed on chain.
    pub submitted_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_submitted_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub certified_hash: String,
    /// Hash of the submission this one follows up on, if any.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub parent: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub published: bool,
    pub synced: bool,
}

fn hash_dir(hash: &str) -> PathBuf {
    config::dir().join(hash)
}

pub fn receipt_path(hash: &str) -> PathBuf {
    hash_dir(hash).join("receipt.json")
}

pub fn manifest_path(hash: &str) -> PathBuf {
    hash_dir(hash).join("anchor-batch.json")
}

/// Directory holding one marker file per allocated local id.
///
/// The markers are the allocation ledger and are never removed, which is what
/// keeps ids monotonic across a deleted submission directory.
fn ids_dir() -> PathBuf {
    config::dir().join(".ids")
}

/// Save a proof document and metadata locally, returning the entry.
pub fn save(hash: &str, proof: &[u8], parent: &str) -> Result<Entry, String> {
    let dir = hash_dir(hash);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create dir: {e}"))?;
    std::fs::write(dir.join("PROOF.md"), proof).map_err(|e| format!("write proof: {e}"))?;

    let meta_path = dir.join("meta.json");
    if meta_path.exists() {
        // Re-saving an existing submission keeps its local id; only fill in a
        // parent that was not known before.
        let mut entry = get_entry(hash)?;
        if entry.local_id == 0 {
            entry.local_id = allocate_local_id(hash)?;
        }
        if !parent.is_empty() && entry.parent.is_empty() {
            entry.parent = parent.to_string();
        }
        save_meta(&meta_path, &entry)?;
        return Ok(entry);
    }

    let entry = Entry {
        hash: hash.to_string(),
        local_id: allocate_local_id(hash)?,
        algorithm: "sha256".to_string(),
        submitted_at: Utc::now(),
        server_submitted_at: None,
        certified_hash: String::new(),
        parent: parent.to_string(),
        title: String::new(),
        description: String::new(),
        published: false,
        synced: false,
    };
    save_meta(&meta_path, &entry)?;
    Ok(entry)
}

/// Load metadata for a hash.
pub fn get_entry(hash: &str) -> Result<Entry, String> {
    let data = std::fs::read_to_string(hash_dir(hash).join("meta.json"))
        .map_err(|e| format!("read meta: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("parse meta: {e}"))
}

/// Update entry metadata.
pub fn update_entry(hash: &str, f: impl FnOnce(&mut Entry)) -> Result<(), String> {
    let mut entry = get_entry(hash)?;
    f(&mut entry);
    save_meta(&hash_dir(hash).join("meta.json"), &entry)
}

/// Get the proof document bytes.
pub fn get_proof(hash: &str) -> Result<Vec<u8>, String> {
    std::fs::read(hash_dir(hash).join("PROOF.md")).map_err(|e| format!("read proof: {e}"))
}

/// Validate and durably retain the private receipt beside its exact report.
pub fn save_receipt(hash: &str, submission_receipt: &SubmissionReceipt) -> Result<(), String> {
    if submission_receipt.document_hash != hash {
        return Err(format!(
            "receipt document hash {} does not match local hash {hash}",
            submission_receipt.document_hash
        ));
    }
    let proof = get_proof(hash)?;
    submission_receipt.validate_report(&proof)?;
    let data = submission_receipt.to_json()?;
    atomic_create(&receipt_path(hash), &data, true, "receipt")
}

/// Read and validate a retained receipt, including its binding to PROOF.md.
pub fn get_receipt(hash: &str) -> Result<SubmissionReceipt, String> {
    let data = std::fs::read(receipt_path(hash)).map_err(|e| format!("read receipt: {e}"))?;
    let submission_receipt = receipt::parse(&data)?;
    if submission_receipt.document_hash != hash {
        return Err(format!(
            "receipt document hash {} does not match local hash {hash}",
            submission_receipt.document_hash
        ));
    }
    submission_receipt.validate_report(&get_proof(hash)?)?;
    Ok(submission_receipt)
}

/// Return the original receipt bytes after validating them. This is used for
/// export so unknown fields and the producer's formatting are not discarded.
pub fn receipt_bytes(hash: &str) -> Result<Vec<u8>, String> {
    let path = receipt_path(hash);
    let data = std::fs::read(&path).map_err(|e| format!("read receipt: {e}"))?;
    let submission_receipt = receipt::parse(&data)?;
    if submission_receipt.document_hash != hash {
        return Err(format!(
            "receipt document hash {} does not match local hash {hash}",
            submission_receipt.document_hash
        ));
    }
    submission_receipt.validate_report(&get_proof(hash)?)?;
    Ok(data)
}

/// Export already-validated receipt bytes without silently replacing a file.
pub fn export_receipt(path: &std::path::Path, data: &[u8]) -> Result<(), String> {
    atomic_create(path, data, true, "receipt")
}

pub fn save_manifest(hash: &str, data: &[u8]) -> Result<(), String> {
    atomic_create(&manifest_path(hash), data, false, "batch manifest")
}

pub fn manifest_bytes(hash: &str) -> Result<Vec<u8>, String> {
    std::fs::read(manifest_path(hash)).map_err(|e| format!("read cached batch manifest: {e}"))
}

/// List all locally stored entries, sorted by submitted_at descending.
pub fn list() -> Result<Vec<Entry>, String> {
    let dir = config::dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(format!("read dir: {e}")),
    };

    let mut result = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_hex(&name) {
            continue;
        }
        if let Ok(e) = get_entry(&name) {
            result.push(e);
        }
    }

    // Submissions predating local ids, or written by an older build, get one
    // now. Oldest first, so the numbering follows the order they were made.
    if result.iter().any(|e| e.local_id == 0) {
        result.sort_by(|a, b| a.submitted_at.cmp(&b.submitted_at));
        for e in result.iter_mut().filter(|e| e.local_id == 0) {
            e.local_id = allocate_local_id(&e.hash)?;
            save_meta(&hash_dir(&e.hash).join("meta.json"), e)?;
        }
    }

    result.sort_by(|a, b| b.submitted_at.cmp(&a.submitted_at));
    Ok(result)
}

/// Resolve a user-supplied reference to a full hash.
///
/// A 64-character hex string is a hash and an all-digit string is a local id;
/// anything else is treated as a hash prefix. Digits therefore never mean two
/// things at once.
pub fn resolve(reference: &str) -> Result<String, String> {
    if reference.is_empty() {
        return Err("empty reference".into());
    }

    if reference.len() == 64 && is_hex(reference) {
        return Ok(reference.to_ascii_lowercase());
    }

    if reference.chars().all(|c| c.is_ascii_digit()) {
        let id: u64 = reference
            .parse()
            .map_err(|_| format!("{reference} is not a usable local id"))?;
        return hash_for_local_id(id).ok_or_else(|| format!("no submission with local id {id}"));
    }

    if !is_hex(reference) {
        return Err(format!("{reference} is neither a hash nor a local id"));
    }

    let prefix = reference.to_ascii_lowercase();
    let matches: Vec<String> = list()?
        .into_iter()
        .map(|e| e.hash)
        .filter(|h| h.starts_with(&prefix))
        .collect();
    match matches.len() {
        0 => Err(format!("no local submission matches {reference}")),
        1 => Ok(matches.into_iter().next().unwrap()),
        n => Err(format!(
            "{reference} matches {n} submissions; use more characters"
        )),
    }
}

/// The hash a local id was assigned to, if that id was ever handed out.
pub fn hash_for_local_id(id: u64) -> Option<String> {
    let hash = std::fs::read_to_string(ids_dir().join(id.to_string())).ok()?;
    let hash = hash.trim().to_string();
    if hash.is_empty() { None } else { Some(hash) }
}

/// Local submissions descending from `hash`, parents before their children.
///
/// Only the parent links recorded locally are followed: a follow-up submitted
/// from another machine is not here to be walked, and could not be published
/// from here anyway without its document.
pub fn descendants(hash: &str) -> Result<Vec<Entry>, String> {
    let mut all = list()?;
    // Oldest first, so each generation comes out in the order it was made.
    all.sort_by(|a, b| a.submitted_at.cmp(&b.submitted_at));

    let mut out: Vec<Entry> = Vec::new();
    let mut frontier = vec![hash.to_string()];
    let mut seen = vec![hash.to_string()];

    while !frontier.is_empty() {
        let mut next = Vec::new();
        for e in &all {
            if !frontier.iter().any(|p| *p == e.parent) || seen.contains(&e.hash) {
                continue;
            }
            seen.push(e.hash.clone());
            next.push(e.hash.clone());
            out.push(get_entry(&e.hash)?);
        }
        frontier = next;
    }
    Ok(out)
}

/// Record the details a finding shows publicly.
pub fn set_public_details(hash: &str, title: &str, description: &str) -> Result<(), String> {
    update_entry(hash, |e| {
        e.title = title.to_string();
        e.description = description.to_string();
    })
}

/// Add a timestamped note.
pub fn add_note(hash: &str, content: &str) -> Result<(), String> {
    let notes_dir = hash_dir(hash).join("notes");
    std::fs::create_dir_all(&notes_dir).map_err(|e| format!("create notes dir: {e}"))?;
    let filename = Utc::now().format("%Y-%m-%d_%H%M%S").to_string() + ".md";
    std::fs::write(notes_dir.join(filename), content).map_err(|e| format!("write note: {e}"))
}

/// List note filenames.
pub fn list_notes(hash: &str) -> Result<Vec<String>, String> {
    let notes_dir = hash_dir(hash).join("notes");
    let entries = match std::fs::read_dir(&notes_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(format!("read notes: {e}")),
    };
    let mut notes: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    notes.sort();
    Ok(notes)
}

/// Claim the next unused local id for `hash`.
///
/// Ids are claimed by creating a marker file exclusively, so two concurrent
/// submits cannot land on the same number, and a marker left behind by a
/// deleted submission keeps that id spent forever. `next` is only a hint: if
/// it is missing or stale the scan starts lower and walks up.
fn allocate_local_id(hash: &str) -> Result<u64, String> {
    let dir = ids_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create id dir: {e}"))?;

    let hint_path = dir.join("next");
    let mut candidate = std::fs::read_to_string(&hint_path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(1)
        .max(1);

    loop {
        let marker = dir.join(candidate.to_string());
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
        {
            Ok(mut f) => {
                use std::io::Write;
                f.write_all(hash.as_bytes())
                    .map_err(|e| format!("write id marker: {e}"))?;
                let _ = std::fs::write(&hint_path, (candidate + 1).to_string());
                return Ok(candidate);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => candidate += 1,
            Err(e) => return Err(format!("claim local id: {e}")),
        }
    }
}

fn save_meta(path: &std::path::Path, entry: &Entry) -> Result<(), String> {
    let data = serde_json::to_string_pretty(entry).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(path, data).map_err(|e| format!("write meta: {e}"))
}

/// Create a file in its final directory, sync its contents, and publish it by
/// rename. Receipts are immutable; an existing valid byte-for-byte receipt is
/// success, while different bytes are never silently overwritten.
fn atomic_create(
    path: &std::path::Path,
    data: &[u8],
    private: bool,
    description: &str,
) -> Result<(), String> {
    if path.exists() {
        let existing =
            std::fs::read(path).map_err(|e| format!("read existing {description}: {e}"))?;
        if existing == data {
            set_file_permissions(path, private, description)?;
            return Ok(());
        }
        return Err(format!(
            "a different {description} already exists; refusing to overwrite it"
        ));
    }

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("vulnhash-evidence");
    let temp = path.with_file_name(format!(
        ".{name}.tmp-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if private { 0o600 } else { 0o644 });
    }

    let result = (|| {
        let mut file = options
            .open(&temp)
            .map_err(|e| format!("create {description} temp file: {e}"))?;
        file.write_all(data)
            .map_err(|e| format!("write {description}: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("sync {description}: {e}"))?;
        drop(file);
        match std::fs::hard_link(&temp, path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = std::fs::read(path)
                    .map_err(|e| format!("read concurrently created {description}: {e}"))?;
                if existing != data {
                    return Err(format!(
                        "a different {description} was created concurrently; refusing to overwrite it"
                    ));
                }
                std::fs::remove_file(&temp)
                    .map_err(|e| format!("remove {description} temp file: {e}"))?;
                set_file_permissions(path, private, description)?;
                return Ok(());
            }
            Err(error) => return Err(format!("publish {description}: {error}")),
        }
        std::fs::remove_file(&temp).map_err(|e| format!("remove {description} temp link: {e}"))?;
        set_file_permissions(path, private, description)?;
        sync_parent(path, description)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(unix)]
fn set_file_permissions(
    path: &std::path::Path,
    private: bool,
    description: &str,
) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if private { 0o600 } else { 0o644 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|e| format!("set {description} permissions: {e}"))
}

#[cfg(not(unix))]
fn set_file_permissions(
    _path: &std::path::Path,
    _private: bool,
    _description: &str,
) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn sync_parent(path: &std::path::Path, description: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    std::fs::File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|e| format!("sync {description} directory: {e}"))
}

#[cfg(not(unix))]
fn sync_parent(_path: &std::path::Path, _description: &str) -> Result<(), String> {
    Ok(())
}

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A store of this test's own, in a directory nothing else touches.
    fn temp_store() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "vulnhash-store-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        config::set_dir_for_test(dir.clone());
        dir
    }

    fn submit(content: &str, parent: &str) -> Entry {
        let hash = hex::encode(Sha256::digest(content.as_bytes()));
        save(&hash, content.as_bytes(), parent).unwrap()
    }

    fn sample_receipt(document_hash: &str) -> SubmissionReceipt {
        let submission_id = "11111111-1111-4111-8111-111111111111";
        let user_id = "22222222-2222-4222-8222-222222222222";
        let submitted_at_unix = 1_786_710_896;
        let username = "researcher";
        SubmissionReceipt {
            schema: receipt::SCHEMA.into(),
            certified_hash_version: receipt::CERTIFIED_HASH_VERSION.into(),
            certified_hash_format: receipt::CERTIFIED_HASH_FORMAT.into(),
            submission_id: submission_id.into(),
            digest_algorithm: receipt::DIGEST_ALGORITHM.into(),
            document_hash: document_hash.into(),
            submitted_at: "2026-08-14T12:34:56Z".into(),
            submitted_at_unix,
            user_id: user_id.into(),
            username: username.into(),
            certified_hash: receipt::certified_hash(
                submission_id,
                document_hash,
                submitted_at_unix,
                user_id,
                username,
            ),
            extra: Default::default(),
        }
    }

    #[test]
    fn local_ids_start_at_one_and_count_up() {
        temp_store();
        assert_eq!(submit("a", "").local_id, 1);
        assert_eq!(submit("b", "").local_id, 2);
        assert_eq!(submit("c", "").local_id, 3);
    }

    // The whole point of the id ledger: a number that named a submission must
    // never come back to name a different one.
    #[test]
    fn a_deleted_submission_does_not_free_its_id() {
        let root = temp_store();
        let first = submit("a", "");
        let second = submit("b", "");

        std::fs::remove_dir_all(root.join(&second.hash)).unwrap();
        let third = submit("c", "");

        assert_eq!(third.local_id, 3, "id {} was reused", third.local_id);
        assert_eq!(first.local_id, 1);
    }

    // Even losing the hint file must not hand the same number out twice.
    #[test]
    fn a_lost_counter_hint_does_not_reissue_ids() {
        let root = temp_store();
        submit("a", "");
        submit("b", "");
        std::fs::remove_file(root.join(".ids").join("next")).unwrap();

        assert_eq!(submit("c", "").local_id, 3);
    }

    #[test]
    fn resubmitting_keeps_the_same_id() {
        temp_store();
        let first = submit("a", "");
        assert_eq!(submit("a", "").local_id, first.local_id);
    }

    #[test]
    fn receipt_is_checked_against_the_saved_proof() {
        let root = temp_store();
        let entry = submit("report", "");
        let valid = sample_receipt(&entry.hash);
        save_receipt(&entry.hash, &valid).unwrap();
        assert_eq!(
            get_receipt(&entry.hash).unwrap().certified_hash,
            valid.certified_hash
        );

        let mut wrong = sample_receipt(&hex::encode(Sha256::digest(b"different")));
        wrong.extra.insert("future".into(), serde_json::json!(true));
        let err = save_receipt(&entry.hash, &wrong).unwrap_err();
        assert!(err.contains("does not match local hash"), "{err}");
        assert!(root.join(&entry.hash).join("receipt.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn receipt_permissions_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_store();
        let entry = submit("report", "");
        save_receipt(&entry.hash, &sample_receipt(&entry.hash)).unwrap();
        let mode = std::fs::metadata(root.join(entry.hash).join("receipt.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn resolve_accepts_an_id_a_prefix_or_a_full_hash() {
        temp_store();
        let entry = submit("a", "");

        assert_eq!(resolve(&entry.local_id.to_string()).unwrap(), entry.hash);
        assert_eq!(resolve(&entry.hash).unwrap(), entry.hash);
        assert_eq!(resolve(&entry.hash.to_uppercase()).unwrap(), entry.hash);
        assert_eq!(resolve(&entry.hash[..10]).unwrap(), entry.hash);
    }

    // Digits are always an id, so a reference never means two things at once.
    #[test]
    fn digits_are_read_as_an_id_not_as_a_hash_prefix() {
        temp_store();
        submit("a", "");
        let err = resolve("99").unwrap_err();
        assert!(err.contains("local id"), "{err}");
    }

    #[test]
    fn resolve_refuses_an_ambiguous_prefix() {
        temp_store();
        // Two entries whose hashes share a first character.
        let mut shared = Vec::new();
        for i in 0..64 {
            let e = submit(&format!("doc-{i}"), "");
            if let Some(other) = shared
                .iter()
                .find(|h: &&String| h.starts_with(&e.hash[..1]))
            {
                let err = resolve(&e.hash[..1]).unwrap_err();
                assert!(err.contains("more characters"), "{err} (vs {other})");
                return;
            }
            shared.push(e.hash);
        }
        panic!("no two hashes shared a leading character");
    }

    #[test]
    fn resolve_rejects_a_reference_that_is_neither() {
        temp_store();
        assert!(resolve("not-a-hash").is_err());
        assert!(resolve("").is_err());
    }

    #[test]
    fn descendants_come_out_parents_first() {
        temp_store();
        let root = submit("root", "");
        let child = submit("child", &root.hash);
        let grandchild = submit("grandchild", &child.hash);
        let sibling = submit("sibling", &root.hash);
        // An unrelated tree must not leak into the walk.
        let other = submit("other", "");
        submit("other-child", &other.hash);

        let found: Vec<String> = descendants(&root.hash)
            .unwrap()
            .into_iter()
            .map(|e| e.hash)
            .collect();
        assert_eq!(found, vec![child.hash, sibling.hash, grandchild.hash]);
    }

    #[test]
    fn descendants_of_a_leaf_are_empty() {
        temp_store();
        let root = submit("root", "");
        assert!(descendants(&root.hash).unwrap().is_empty());
    }

    // Submissions written before local ids existed get one on first listing,
    // oldest first, so the numbering matches the order they were made.
    #[test]
    fn entries_without_an_id_are_backfilled_oldest_first() {
        let root = temp_store();
        let old = submit("old", "");
        let new = submit("new", "");

        for (hash, ts) in [
            (&old.hash, "2020-01-01T00:00:00Z"),
            (&new.hash, "2021-01-01T00:00:00Z"),
        ] {
            let meta = root.join(hash).join("meta.json");
            let mut value: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&meta).unwrap()).unwrap();
            value["local_id"] = serde_json::json!(0);
            value["submitted_at"] = serde_json::json!(ts);
            std::fs::write(&meta, value.to_string()).unwrap();
        }

        let listed = list().unwrap();
        let by_hash = |h: &str| listed.iter().find(|e| e.hash == h).unwrap().local_id;
        assert!(
            by_hash(&old.hash) < by_hash(&new.hash),
            "backfill ignored submission order"
        );
        // Ids already spent are not handed out again.
        assert!(by_hash(&old.hash) > 2, "backfill reused a spent id");
    }
}
