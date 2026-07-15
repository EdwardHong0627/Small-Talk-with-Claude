use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use blog_api::ratelimit::RateLimiter;
use blog_api::{build_app, config::Config, db, AppState};
use tower_http::cors::{Any, CorsLayer};

/// Default rate-limit: requests allowed per (client_ip, route) per window.
const RATE_LIMIT_REQUESTS: u32 = 30;
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

#[tokio::main]
async fn main() {
    init_tracing();

    let migrate_only = std::env::args().any(|a| a == "--migrate-only");

    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("configuration error: {e}");
            std::process::exit(1);
        }
    };

    let conn = db::open(&config.db_path).unwrap_or_else(|e| {
        eprintln!("failed to open database at {}: {e}", config.db_path);
        std::process::exit(1);
    });
    db::migrate(&conn).unwrap_or_else(|e| {
        eprintln!("migration failed: {e}");
        std::process::exit(1);
    });

    if migrate_only {
        tracing::info!("migrations applied, exiting (--migrate-only)");
        return;
    }

    if config.auto_approve {
        tracing::warn!("BLOG_API_AUTO_APPROVE is on -- comments publish without moderation");
    }

    let state = AppState {
        conn: Arc::new(Mutex::new(conn)),
        admin_token: config.admin_token.clone(),
        rate_limiter: Arc::new(RateLimiter::new(RATE_LIMIT_REQUESTS, RATE_LIMIT_WINDOW)),
        auto_approve: config.auto_approve,
    };

    let mut app = build_app(state);

    if let Some(origin) = &config.dev_cors_origin {
        tracing::warn!(origin = %origin, "dev CORS enabled -- do not use in production");
        match origin.parse::<axum::http::HeaderValue>() {
            Ok(header_value) => {
                let cors = CorsLayer::new()
                    .allow_origin(header_value)
                    .allow_methods(Any)
                    .allow_headers(Any);
                app = app.layer(cors);
            }
            Err(e) => {
                eprintln!("invalid BLOG_API_DEV_CORS_ORIGIN {origin:?}: {e}");
                std::process::exit(1);
            }
        }
    }

    let addr: SocketAddr = config.bind_addr.parse().unwrap_or_else(|e| {
        eprintln!("invalid BLOG_API_BIND_ADDR {:?}: {e}", config.bind_addr);
        std::process::exit(1);
    });

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("failed to bind {addr}: {e}");
            std::process::exit(1);
        });

    tracing::info!(%addr, "blog-api listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
