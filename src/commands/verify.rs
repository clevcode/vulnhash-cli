use crate::{config, client::Client, store};

pub fn run(reference: &str, tor: Option<bool>) -> Result<(), String> {
    let hash = store::resolve(reference)?;
    let cfg = config::load();
    if cfg.auth_token.is_empty() {
        return Err("not configured. Run 'vulnhash config' first.".into());
    }

    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    rt.block_on(async {
        let client = Client::connect(&cfg, tor, false).await?;

        let resp = client.verify_hash(&hash).await?;
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
