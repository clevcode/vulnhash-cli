use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config;

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub hash: String,
    pub algorithm: String,
    pub submitted_at: DateTime<Utc>,
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

/// Save a proof document and metadata locally.
pub fn save(hash: &str, proof: &[u8]) -> Result<(), String> {
    let dir = hash_dir(hash);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create dir: {e}"))?;
    std::fs::write(dir.join("PROOF.md"), proof).map_err(|e| format!("write proof: {e}"))?;

    let meta_path = dir.join("meta.json");
    if !meta_path.exists() {
        let entry = Entry {
            hash: hash.to_string(),
            algorithm: "sha256".to_string(),
            submitted_at: Utc::now(),
            title: String::new(),
            description: String::new(),
            published: false,
            synced: false,
        };
        save_meta(&meta_path, &entry)?;
    }
    Ok(())
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
    std::fs::read(hash_dir(hash).join("PROOF.md"))
        .map_err(|e| format!("read proof: {e}"))
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

    result.sort_by(|a, b| b.submitted_at.cmp(&a.submitted_at));
    Ok(result)
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

fn save_meta(path: &std::path::Path, entry: &Entry) -> Result<(), String> {
    let data = serde_json::to_string_pretty(entry).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(path, data).map_err(|e| format!("write meta: {e}"))
}

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
}
