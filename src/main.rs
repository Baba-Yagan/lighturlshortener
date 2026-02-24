use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Redirect,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    sync::{Arc, RwLock},
};

const DB_FILE: &str = "urls.json";

#[derive(Clone, Serialize, Deserialize)]
struct UrlEntry {
    short: String,
    long: String,
}

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
        .route("/update", post(update_url))
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
    "Welcome to the URL Shortener. POST to /update with {\"short\": \"code\", \"long\": \"url\"} to add."
}

async fn redirect(State(state): State<AppState>, Path(code): Path<String>) -> Result<Redirect, StatusCode> {
    let db = state.db.read().await;
    if let Some(url) = db.get(&code) {
        Ok(Redirect::permanent(url))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn update_url(
    State(state): State<AppState>,
    Json(entry): Json<UrlEntry>,
) -> StatusCode {
    let mut db = state.db.write().await;
    db.insert(entry.short.clone(), entry.long);
    
    // Write to disk only on update
    save_db(&db);
    
    StatusCode::CREATED
}
