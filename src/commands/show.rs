use crate::store;

pub fn run(hash: &str) -> Result<(), String> {
    let entry = store::get_entry(hash)?;
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

    let notes = store::list_notes(hash)?;
    if !notes.is_empty() {
        println!("\nNotes ({}):", notes.len());
        for n in notes {
            println!("  - {n}");
        }
    }
    Ok(())
}
