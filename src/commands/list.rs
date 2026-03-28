use crate::store;

pub fn run() -> Result<(), String> {
    let entries = store::list()?;
    if entries.is_empty() {
        println!("No submissions found.");
        return Ok(());
    }

    println!("{:<64}  {:<10}  {:<6}  {}", "HASH", "STATUS", "SYNCED", "DATE");
    println!("{}", "-".repeat(100));
    for e in entries {
        let status = if e.published { "published" } else { "private" };
        let synced = if e.synced { "yes" } else { "no" };
        let date = e.submitted_at.format("%Y-%m-%d %H:%M");
        println!("{:<64}  {:<10}  {:<6}  {}", e.hash, status, synced, date);
    }
    Ok(())
}
