use crate::{output, store};

pub fn run(reference: &str, json: bool) -> Result<(), String> {
    let hash = store::resolve(reference)?;
    let entry = store::get_entry(&hash)?;

    if json {
        let parent_id = store::get_entry(&entry.parent).ok().map(|p| p.local_id);
        let mut value = output::entry(&entry, parent_id);
        value["notes"] = serde_json::json!(store::list_notes(&hash)?);
        value["follow_ups"] = serde_json::Value::Array(
            store::descendants(&hash)?
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "id": c.local_id,
                        "hash": c.hash,
                        "published": c.published,
                    })
                })
                .collect(),
        );
        output::print(&value);
        return Ok(());
    }

    println!("ID:          {}", entry.local_id);
    println!("Hash:        {}", entry.hash);
    println!("Algorithm:   {}", entry.algorithm);
    println!(
        "Saved local: {}",
        entry.submitted_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    if let Some(server_time) = entry.server_submitted_at {
        println!(
            "Server time: {}",
            server_time.format("%Y-%m-%d %H:%M:%S UTC")
        );
    }
    if !entry.certified_hash.is_empty() {
        println!("Certified:  {}", entry.certified_hash);
    }
    println!("Published:   {}", entry.published);
    println!("Synced:      {}", entry.synced);
    if !entry.title.is_empty() {
        println!("Title:       {}", entry.title);
    }
    if !entry.description.is_empty() {
        println!("Description: {}", entry.description);
    }
    if !entry.parent.is_empty() {
        match store::get_entry(&entry.parent) {
            Ok(parent) => println!("Follows up on: {} {}", parent.local_id, parent.hash),
            Err(_) => println!("Follows up on: {}", entry.parent),
        }
    }

    let children = store::descendants(&hash)?;
    if !children.is_empty() {
        println!("\nFollow-ups ({}):", children.len());
        for c in children {
            let status = if c.published { "published" } else { "private" };
            println!("  {:>4}  {}  {}", c.local_id, c.hash, status);
        }
    }

    let notes = store::list_notes(&hash)?;
    if !notes.is_empty() {
        println!("\nNotes ({}):", notes.len());
        for n in notes {
            println!("  - {n}");
        }
    }
    Ok(())
}
