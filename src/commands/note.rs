use std::io::Read;
use crate::store;

pub fn run(hash: &str, text: Option<&str>) -> Result<(), String> {
    let content = match text {
        Some(t) => t.to_string(),
        None => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).map_err(|e| format!("read stdin: {e}"))?;
            buf
        }
    };

    if content.is_empty() {
        return Err("note content is empty".into());
    }

    store::add_note(hash, &content)?;
    println!("Note added.");
    Ok(())
}
