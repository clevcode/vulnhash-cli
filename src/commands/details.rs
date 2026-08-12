use crate::{client::Client, config, output, store};

/// Set the details a finding shows publicly.
///
/// A registered finding is on the public record from the moment it is made:
/// its certified hash, when it arrived, and whose it is. What it is about is
/// not, unless the researcher says so. These details are that channel — a way
/// to stake out "I have something in this area" without disclosing the
/// document, and they can be set and changed long before publication.
///
/// Saved locally first. The researcher's own copy is the one that must survive
/// a network failure, and an upload that fails is reported rather than
/// swallowed, since details nobody can see are not doing their job.
pub fn run(
    reference: &str,
    title: Option<&str>,
    description: Option<&str>,
    tor: Option<bool>,
    json: bool,
) -> Result<(), String> {
    let hash = store::resolve(reference)?;
    let entry = store::get_entry(&hash)?;

    // Absent means "leave as it is"; an empty string means "clear it".
    let title = title.unwrap_or(&entry.title).to_string();
    let description = description.unwrap_or(&entry.description).to_string();

    store::set_public_details(&hash, &title, &description)?;

    let cfg = config::load();
    if cfg.auth_token.is_empty() {
        if json {
            output::print(&serde_json::json!({
                "id": entry.local_id,
                "hash": hash,
                "title": title,
                "description": description,
                "visible": false,
                "server_error": "no auth token configured",
            }));
        } else {
            println!("Saved locally for {} {}", entry.local_id, hash);
            println!("Not visible to anyone: no auth token configured.");
        }
        return Ok(());
    }

    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    rt.block_on(async {
        let client = Client::connect(&cfg, tor, false).await?;
        let outcome = client.update_hash(&hash, &title, &description).await;

        if json {
            output::print(&serde_json::json!({
                "id": entry.local_id,
                "hash": hash,
                "title": title,
                "description": description,
                "visible": outcome.is_ok(),
                "server_error": match &outcome {
                    Ok(()) => serde_json::Value::Null,
                    Err(e) => serde_json::json!(e),
                },
            }));
            return Ok(());
        }

        match outcome {
            Ok(()) => {
                println!("Public details set for {} {}", entry.local_id, hash);
                if !title.is_empty() {
                    println!("  title:       {title}");
                }
                if !description.is_empty() {
                    println!("  description: {description}");
                }
                println!("Visible to anyone browsing your findings, disclosed or not.");
            }
            Err(e) => {
                eprintln!("warning: saved locally, but the server refused: {e}");
            }
        }
        Ok(())
    })
}
