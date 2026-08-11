use crate::{config, client::Client, output, store};

pub fn run(reference: &str, tor: Option<bool>, json: bool) -> Result<(), String> {
    let hash = store::resolve(reference)?;

    // No account needed. Checking whether a hash was recorded, and when, is
    // the one thing a stranger has to be able to do — the triager weighing a
    // researcher's claim is exactly the person without a token. The endpoint
    // is public and both transports omit the header when the token is empty.
    let cfg = config::load();

    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    rt.block_on(async {
        let client = Client::connect(&cfg, tor, false).await?;

        let resp = client.verify_hash(&hash).await?;

        if json {
            output::print(&serde_json::json!({
                "hash": resp.hash,
                "exists": resp.exists,
                "published": resp.published,
                "submitted_at": resp.submitted_at,
                "username": resp.username,
            }));
            return Ok(());
        }

        if resp.exists {
            println!("Hash:        {}", resp.hash);
            println!("Status:      FOUND");
            if let Some(ts) = &resp.submitted_at {
                println!("Submitted:   {ts}");
            }
            println!("Published:   {}", resp.published);
            if let Some(user) = &resp.username {
                println!("By:          {user}");
            }
        } else {
            println!("Hash: {hash}");
            println!("Status: NOT FOUND");
        }
        Ok(())
    })
}
