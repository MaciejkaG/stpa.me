/*
    stpa.me is the web server behind the Short Links feature of Starpaste.
    Copyright (C) 2025 Maciej "mcjk" Gomoła

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU Affero General Public License as published
    by the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Affero General Public License for more details.

    You should have received a copy of the GNU Affero General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::get,
};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::{collections::HashMap, env, fs::File, path::Path as StdPath, sync::Arc, time::Duration};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{info, warn};

#[derive(Clone)]
struct AppState {
    db: PgPool,
    default_redirect: String,
    csv_links: Arc<HashMap<String, String>>,
    link_cache: Cache<String, ShortLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ShortLink {
    id: uuid::Uuid,
    token: String,
    long_url: String,
    created_at: chrono::DateTime<chrono::Utc>,
    click_count: i64,
    is_active: bool,
    #[serde(default)]
    source: LinkSource,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
enum LinkSource {
    #[default]
    Database,
    Csv,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Load configuration from environment
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://localhost/starpaste".to_string());

    let default_redirect =
       env::var("DEFAULT_REDIRECT_URL").unwrap_or_else(|_| "https://starpaste.eu".to_string());

    let bind_address = env::var("BIND_ADDRESS").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
    info!("Starting short link server...");
    info!("Database URL: {}", database_url);
    info!("Default redirect: {}", default_redirect);
    info!("Bind address: {}", bind_address);

    // Connect to database
    info!("Attempting to connect to database...");
    let pool = match PgPool::connect(&database_url).await {
        Ok(pool) => {
            info!("Successfully connected to database");
            pool
        }
        Err(e) => {
            warn!("Failed to connect to database: {}", e);
            warn!("Make sure PostgreSQL is running and accessible at: {}", database_url);
            return Err(e.into());
        }
    };

    // Load CSV links at startup
    let csv_links = Arc::new(read_csv_links());
    info!("Loaded {} links from CSV file into memory", csv_links.len());

    // Create a cache for database lookups with 5 minute TTL and max 10000 entries
    let link_cache: Cache<String, ShortLink> = Cache::builder()
        .max_capacity(10_000)
        .time_to_live(Duration::from_secs(300))
        .build();

    let state = AppState {
        db: pool,
        default_redirect,
        csv_links,
        link_cache,
    };

    // Build router
    let app = Router::new()
        .route("/", get(handle_root))
        .route("/:token", get(handle_redirect))
        .route("/health", get(health_check))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    info!("Server starting on {}", bind_address);

    let listener = tokio::net::TcpListener::bind(&bind_address).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn handle_root(State(state): State<AppState>) -> impl IntoResponse {
    info!("Root redirect to: {}", state.default_redirect);
    Redirect::permanent(&state.default_redirect)
}

async fn handle_redirect(
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    // First, check the in-memory cache
    if let Some(link) = state.link_cache.get(&token).await {
        // Increment click count asynchronously only for database entries
        if link.source == LinkSource::Database {
            let db_clone = state.db.clone();
            let token_clone = token.clone();
            tokio::spawn(async move {
                if let Err(e) = increment_click_count(&db_clone, &token_clone).await {
                    warn!("Failed to increment click count for {}: {}", token_clone, e);
                }
            });
        }

        info!("Cache hit: Redirecting {} to {}", token, link.long_url);
        return Redirect::permanent(&link.long_url).into_response();
    }

    match get_short_link(&state.db, &token, &state.csv_links).await {
        Ok(Some(link)) => {
            let long_url = link.long_url.clone();
            let source = link.source;
            
            // Store in cache for future requests
            state.link_cache.insert(token.clone(), link).await;

            // Increment click count asynchronously only for database entries
            if source == LinkSource::Database {
                let db_clone = state.db.clone();
                let token_clone = token.clone();
                tokio::spawn(async move {
                    if let Err(e) = increment_click_count(&db_clone, &token_clone).await {
                        warn!("Failed to increment click count for {}: {}", token_clone, e);
                    }
                });
            }

            info!("Cache miss: Redirecting {} to {}", token, long_url);
            Redirect::permanent(&long_url).into_response()
        }
        Ok(None) => {
            warn!("Token not found: {}", token);
            (StatusCode::NOT_FOUND, "Short link not found").into_response()
        }
        Err(e) => {
            warn!("Database error for token {}: {}", token, e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
        }
    }
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

/// Read CSV file from the root directory and return a HashMap of token -> URL mappings
fn read_csv_links() -> HashMap<String, String> {
    let csv_path = "links.csv";
    
    // Check if the CSV file exists
    if !StdPath::new(csv_path).exists() {
        info!("CSV file {} not found, skipping CSV lookup", csv_path);
        return HashMap::new();
    }

    match File::open(csv_path) {
        Ok(file) => {
            let mut reader = csv::Reader::from_reader(file);
            let mut links = HashMap::new();

            for result in reader.records() {
                match result {
                    Ok(record) => {
                        if record.len() >= 2 {
                            let token = record.get(0).unwrap_or("").trim().to_string();
                            let url = record.get(1).unwrap_or("").trim().to_string();
                            
                            if !token.is_empty() && !url.is_empty() {
                                links.insert(token, url);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Error reading CSV record: {}", e);
                    }
                }
            }

            info!("Loaded {} links from CSV file", links.len());
            links
        }
        Err(e) => {
            warn!("Failed to open CSV file {}: {}", csv_path, e);
            HashMap::new()
        }
    }
}

async fn get_short_link(pool: &PgPool, token: &str, csv_links: &HashMap<String, String>) -> anyhow::Result<Option<ShortLink>> {
    // First, try to get the link from the database
    let row = sqlx::query(
        "SELECT id, token, long_url, created_at, click_count, is_active 
         FROM short_links 
         WHERE token = $1 AND is_active = true",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => Ok(Some(ShortLink {
            id: row.get("id"),
            token: row.get("token"),
            long_url: row.get("long_url"),
            created_at: row.get("created_at"),
            click_count: row.get("click_count"),
            is_active: row.get("is_active"),
            source: LinkSource::Database,
        })),
        None => {
            // If not found in database, check pre-loaded CSV links
            if let Some(long_url) = csv_links.get(token) {
                info!("Found token {} in CSV links, redirecting to {}", token, long_url);
                
                // Create a ShortLink struct for CSV entries
                // Note: id, created_at are placeholders since CSV entries don't have database records
                Ok(Some(ShortLink {
                    id: uuid::Uuid::nil(), // Use nil UUID to indicate this is not a real DB entry
                    token: token.to_string(),
                    long_url: long_url.clone(),
                    created_at: chrono::DateTime::UNIX_EPOCH, // Placeholder timestamp
                    click_count: 0, // CSV entries don't track clicks
                    is_active: true,
                    source: LinkSource::Csv,
                }))
            } else {
                Ok(None)
            }
        }
    }
}

async fn increment_click_count(pool: &PgPool, token: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE short_links 
         SET click_count = click_count + 1, updated_at = NOW() 
         WHERE token = $1",
    )
    .bind(token)
    .execute(pool)
    .await?;


    Ok(())
}
