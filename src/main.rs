use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use rand::{distributions::Alphanumeric, Rng};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    sync::Arc,
};
use tokio::sync::RwLock;

const DB_FILE: &str = "urls.json";

#[derive(Deserialize)]
struct AddParams {
    token: String,
    #[serde(default)]
    short: Option<String>,
    #[serde(default)]
    long: Option<String>,
}

#[derive(Clone)]
struct AppState {
    // Using RwLock allows multiple concurrent readers (redirects)
    // but exclusive access for writers (updates).
    db: Arc<RwLock<HashMap<String, String>>>,
}

#[tokio::main]
async fn main() {
    // 1. Load data from disk into memory
    let initial_map = load_db();
    let state = AppState {
        db: Arc::new(RwLock::new(initial_map)),
    };

    // 2. Setup Router
    let app = Router::new()
        .route("/", get(root))
        .route("/:code", get(redirect))
        .route("/add", get(add_url_via_get))
        .with_state(state);

    // 3. Bind to specified address and port
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("{}:{}", host, port);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("Listening on http://{}", addr);
    axum::serve(listener, app).await.unwrap();
}

fn load_db() -> HashMap<String, String> {
    if let Ok(content) = fs::read_to_string(DB_FILE) {
        if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&content) {
            return map;
        }
    }
    HashMap::new()
}

fn save_db(map: &HashMap<String, String>) {
    if let Ok(json) = serde_json::to_string_pretty(map) {
        let _ = fs::write(DB_FILE, json);
    }
}

async fn root() -> &'static str {
    "Welcome to the URL Shortener. 
    GET /:code to redirect.
    GET /add?token=SECRET&short=code&long=url to add."
}

async fn redirect(State(state): State<AppState>, Path(code): Path<String>) -> Result<Html<String>, StatusCode> {
    let db = state.db.read().await;
    if let Some(url) = db.get(&code) {
        // Ensure the URL has a protocol to prevent relative redirects
        let target_url = if url.starts_with("http://") || url.starts_with("https://") {
            url.clone()
        } else {
            format!("http://{}", url)
        };

        let html = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Redirecting...</title>
    <meta http-equiv="refresh" content="5;url={}" />
    <script>
        setTimeout(function(){{
            window.location.href = "{}";
        }}, 5000);
    </script>
</head>
<body>
    <p>Redirecting in 5 seconds...</p>
    <noscript>
        <p>Click <a href="{}">here</a> if you are not redirected automatically.</p>
    </noscript>
</body>
</html>"#,
            target_url, target_url, target_url
        );
        Ok(Html(html))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn add_url_via_get(
    Query(params): Query<AddParams>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let expected_token = std::env::var("ADD_TOKEN").unwrap_or_else(|_| "".to_string());
    
    if expected_token.is_empty() {
        return Html("<p>Error: ADD_TOKEN environment variable not set.</p>".to_string());
    }

    if params.token != expected_token {
        return Html("<p>Error: Invalid token.</p>".to_string());
    }

    // If long URL is missing or empty, show the HTML form
    if params.long.is_none() || params.long.as_ref().unwrap().is_empty() {
        let html = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Shorten URL</title>
</head>
<body>
    <h2>Shorten URL</h2>
    <form action="/add" method="get">
        <input type="hidden" name="token" value="{}" />
        <label for="long">Long URL:</label><br>
        <input type="text" id="long" name="long" style="width: 300px;" required /><br><br>
        <button type="submit">Shorten</button>
    </form>
</body>
</html>"#,
            params.token
        );
        return Html(html);
    }

    let long_url = params.long.unwrap();

    let mut db = state.db.write().await;

    // Check if the URL already exists in the database
    if let Some((existing_code, _)) = db.iter().find(|(_, v)| *v == &long_url) {
        // Return the existing short code if found
        return Html(format!("http://127.0.0.1:3000/{}", existing_code));
    }

    // Generate a short code if one wasn't provided
    let short_code = params.short.unwrap_or_else(|| {
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(6)
            .map(char::from)
            .collect()
    });

    db.insert(short_code.clone(), long_url);
    
    // Write to disk only on update
    save_db(&db);
    
    Html(format!("http://127.0.0.1:3000/{}", short_code))
}
