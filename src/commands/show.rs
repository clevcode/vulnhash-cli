use crate::store;

pub fn run(reference: &str) -> Result<(), String> {
    let hash = store::resolve(reference)?;
    let entry = store::get_entry(&hash)?;
    println!("ID:          {}", entry.local_id);
    println!("Hash:        {}", entry.hash);
    println!("Algorithm:   {}", entry.algorithm);
    println!("Submitted:   {}", entry.submitted_at.format("%Y-%m-%d %H:%M:%S UTC"));
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
