use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};

use crate::{config, client, store};

// Embedded bundle files
const INDEX_HTML: &str = include_str!(concat!(env!("BUNDLE_DIR"), "/index.html"));
const MAIN_JS: &[u8] = include_bytes!(concat!(env!("BUNDLE_DIR"), "/main.js"));

pub fn run(port: u16) -> Result<(), String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    rt.block_on(async {
        let app = Router::new()
            // API routes
            .route("/api/entries", get(handle_list_entries))
            .route("/api/entries/{hash}", get(handle_get_entry))
            .route("/api/entries/{hash}/proof", get(handle_get_proof))
            .route("/api/entries/{hash}/notes", get(handle_list_notes).post(handle_add_note))
            .route("/api/entries/{hash}/publish", post(handle_publish))
            .route("/api/entries/{hash}/details", axum::routing::put(handle_set_details))
            .route("/api/config", get(handle_get_config).put(handle_set_config))
            // Static files
            .route("/main.js", get(serve_js))
            .fallback(get(serve_index));

        let addr = format!("0.0.0.0:{port}");
        println!("VulnHash local server running at http://localhost:{port}");
        println!("Press Ctrl+C to stop.");

        let listener = tokio::net::TcpListener::bind(&addr).await
            .map_err(|e| format!("bind: {e}"))?;
        axum::serve(listener, app).await
            .map_err(|e| format!("server: {e}"))
    })
}

async fn serve_index() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], INDEX_HTML)
}

async fn serve_js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], MAIN_JS)
}

async fn handle_list_entries() -> impl IntoResponse {
    match store::list() {
        Ok(entries) => Json(serde_json::json!(entries)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn handle_get_entry(Path(hash): Path<String>) -> impl IntoResponse {
    match store::get_entry(&hash) {
        Ok(entry) => Json(serde_json::json!(entry)).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "not found"}))).into_response(),
    }
}

async fn handle_get_proof(Path(hash): Path<String>) -> impl IntoResponse {
    match store::get_proof(&hash) {
        Ok(proof) => ([(header::CONTENT_TYPE, "text/markdown; charset=utf-8")], proof).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "proof not found"}))).into_response(),
    }
}

async fn handle_list_notes(Path(hash): Path<String>) -> impl IntoResponse {
    let notes = store::list_notes(&hash).unwrap_or_default();
    let notes_dir = config::dir().join(&hash).join("notes");

    #[derive(serde::Serialize)]
    struct NoteEntry { written_at: String, content: String }

    let result: Vec<NoteEntry> = notes.into_iter().filter_map(|n| {
        let content = std::fs::read_to_string(notes_dir.join(&n)).ok()?;
        Some(NoteEntry { written_at: note_timestamp(&n), content })
    }).collect();

    Json(serde_json::json!(result))
}

/// Recover the moment a note was written from the name it was filed under.
///
/// Notes are stored as 2026-03-28_223323.md. The name is how the store keeps
/// them ordered and unique; what a reader wants is the time, so hand back an
/// RFC 3339 timestamp and let the caller format it. A name that does not parse
/// is passed through rather than dropped — better an odd label than a note
/// that appears to have come from nowhere.
fn note_timestamp(filename: &str) -> String {
    let stem = filename.strip_suffix(".md").unwrap_or(filename);
    match stem.split_once('_') {
        Some((date, time)) if date.len() == 10 && time.len() == 6 => {
            format!("{date}T{}:{}:{}Z", &time[0..2], &time[2..4], &time[4..6])
        }
        _ => stem.to_string(),
    }
}

/// Set the details this finding shows publicly, locally and on the server.
///
/// Local first: the researcher's own copy is the record that must not be lost
/// if the network is down. The upload is attempted after, and a failure is
/// reported rather than hidden, since the point of these details is that other
/// people can see them.
async fn handle_set_details(
    Path(hash): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let title = body["title"].as_str().unwrap_or("").to_string();
    let description = body["description"].as_str().unwrap_or("").to_string();

    if let Err(e) = store::set_public_details(&hash, &title, &description) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response();
    }

    let cfg = config::load();
    if cfg.auth_token.is_empty() {
        return Json(serde_json::json!({
            "status": "saved_locally",
            "warning": "no auth token configured, so these details are not visible to anyone yet",
        })).into_response();
    }

    let client = client::Client::new(&cfg);
    match client.update_hash(&hash, &title, &description).await {
        Ok(()) => Json(serde_json::json!({"status": "published"})).into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"status": "saved_locally", "error": e})),
        ).into_response(),
    }
}

async fn handle_add_note(
    Path(hash): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let content = body["content"].as_str().unwrap_or("");
    match store::add_note(&hash, content) {
        Ok(()) => Json(serde_json::json!({"status": "ok"})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn handle_publish(Path(hash): Path<String>) -> impl IntoResponse {
    let proof = match store::get_proof(&hash) {
        Ok(p) => p,
        Err(_) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "proof not found"}))).into_response(),
    };

    // Verify hash
    use sha2::{Sha256, Digest};
    let computed = hex::encode(Sha256::digest(&proof));
    if computed != hash {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "hash mismatch"}))).into_response();
    }

    let cfg = config::load();
    if cfg.auth_token.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "not configured"}))).into_response();
    }

    let content = String::from_utf8_lossy(&proof).to_string();
    let c = client::Client::new(&cfg);
    match c.publish_hash(&hash, &content).await {
        Ok(()) => {
            let _ = store::update_entry(&hash, |e| e.published = true);
            Json(serde_json::json!({"status": "published"})).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn handle_get_config() -> impl IntoResponse {
    let cfg = config::load();
    let masked = if cfg.auth_token.len() > 10 {
        format!("{}...{}", &cfg.auth_token[..6], &cfg.auth_token[cfg.auth_token.len()-4..])
    } else if !cfg.auth_token.is_empty() {
        "(set)".to_string()
    } else {
        String::new()
    };
    Json(serde_json::json!({"server": cfg.server, "auth_token": masked}))
}

async fn handle_set_config(Json(body): Json<serde_json::Value>) -> impl IntoResponse {
    let mut cfg = config::load();
    if let Some(s) = body["server"].as_str() {
        if !s.is_empty() { cfg.server = s.to_string(); }
    }
    if let Some(t) = body["auth_token"].as_str() {
        if !t.is_empty() { cfg.auth_token = t.to_string(); }
    }
    match cfg.save() {
        Ok(()) => Json(serde_json::json!({"status": "saved"})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}
