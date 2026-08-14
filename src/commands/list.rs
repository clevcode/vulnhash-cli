use crate::{output, store};

pub fn run(json: bool) -> Result<(), String> {
    let entries = store::list()?;

    if json {
        output::print(&output::entries(&entries));
        return Ok(());
    }

    if entries.is_empty() {
        println!("No submissions found.");
        return Ok(());
    }

    println!(
        "{:>4}  {:<64}  {:>8}  {:<9}  {:<6}  DATE",
        "ID", "HASH", "PARENT", "STATUS", "SYNCED"
    );
    println!("{}", "-".repeat(110));
    for e in &entries {
        let status = if e.published { "published" } else { "private" };
        let synced = if e.synced { "yes" } else { "no" };
        let date = e.submitted_at.format("%Y-%m-%d %H:%M");
        // A parent submitted from another machine has no local id here, so
        // name it by the start of its hash rather than showing nothing.
        let parent = match entries.iter().find(|p| p.hash == e.parent) {
            Some(p) => p.local_id.to_string(),
            None if e.parent.is_empty() => String::new(),
            None => e.parent[..8].to_string(),
        };
        println!(
            "{:>4}  {:<64}  {:>8}  {:<9}  {:<6}  {}",
            e.local_id, e.hash, parent, status, synced, date
        );
    }

    println!("\nThe ID works anywhere a hash does: vulnhash show 3, vulnhash publish 3");
    Ok(())
}
