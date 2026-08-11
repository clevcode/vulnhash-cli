use std::io::Read;
use crate::{output, store};

pub fn run(reference: &str, text: Option<&str>, json: bool) -> Result<(), String> {
    let hash = store::resolve(reference)?;
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

    store::add_note(&hash, &content)?;
    if json {
        let entry = store::get_entry(&hash)?;
        output::print(&serde_json::json!({
            "status": "ok",
            "id": entry.local_id,
            "hash": hash,
            "notes": store::list_notes(&hash)?,
        }));
    } else {
        println!("Note added.");
    }
    Ok(())
}
