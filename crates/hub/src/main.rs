mod controllers;
mod models;
mod proxy;
mod routes;
mod store;

use actix_web::{App, HttpRequest, HttpResponse, HttpServer, middleware, web};
use clap::Parser;
use controllers::tokens::{SharedStore, extract_bearer};
use models::{AdminToken, UpstreamUrl};
use reqwest::Client;
use std::path::PathBuf;
use std::sync::Arc;
use store::TokenStore;

#[derive(Parser, Debug)]
#[command(
    name = "oxen-hub",
    about = "Oxen Hub reverse proxy with token-based namespace authorization"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    Start {
        #[arg(long, default_value = "http://localhost:3000")]
        upstream: String,

        #[arg(long, default_value_t = 4000)]
        port: u16,

        #[arg(long, default_value = "./hub_data")]
        sync_dir: PathBuf,
    },
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let args = Args::parse();
    let Command::Start {
        upstream,
        port,
        sync_dir,
    } = args.command;

    std::fs::create_dir_all(&sync_dir)?;

    let admin_token = std::env::var("OXEN_HUB_ADMIN_TOKEN").map_err(|_| {
        std::io::Error::other("OXEN_HUB_ADMIN_TOKEN environment variable must be set")
    })?;

    let db_path = sync_dir.join("tokens.db");
    let token_store =
        TokenStore::open(&db_path).map_err(|e| std::io::Error::other(e.to_string()))?;

    // Rotate admin on every restart: remove stale token: key before writing new one.
    let _ = token_store.delete_by_name("__admin__");
    token_store
        .insert(&models::TokenRecord {
            name: "__admin__".to_string(),
            token: admin_token.clone(),
            namespaces: vec!["*".to_string()],
        })
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let shared_store: SharedStore = Arc::new(token_store);
    let shared_store_data = web::Data::new(shared_store);
    let admin_token_data = web::Data::new(AdminToken(admin_token));
    let upstream_data = web::Data::new(UpstreamUrl(upstream.clone()));
    let http_client = web::Data::new(
        Client::builder()
            .build()
            .map_err(|e| std::io::Error::other(e.to_string()))?,
    );

    log::info!("oxen-hub listening on 0.0.0.0:{port}, proxying to {upstream}");

    HttpServer::new(move || {
        let store = shared_store_data.clone();
        let admin = admin_token_data.clone();
        let upstream = upstream_data.clone();
        let client = http_client.clone();

        App::new()
            .wrap(middleware::Logger::default())
            .app_data(store.clone())
            .app_data(admin.clone())
            .app_data(upstream.clone())
            .app_data(client.clone())
            .app_data(web::PayloadConfig::new(2 * 1024 * 1024 * 1024))
            .configure(routes::configure)
            .route(
                "/healthz",
                web::get().to(|| async { HttpResponse::Ok().body("ok") }),
            )
            .default_service(web::to(move |req: HttpRequest, body: web::Bytes| {
                let upstream_inner = upstream.clone();
                let client_inner = client.clone();
                let store_inner = store.clone();
                async move {
                    let token = extract_bearer(&req);
                    let path = req.uri().path().to_string();
                    let namespace = extract_namespace(&path).map(|s| s.to_string());

                    let authorized = check_auth(token, namespace.as_deref(), &store_inner).await;
                    if !authorized {
                        return Ok::<HttpResponse, actix_web::Error>(
                            HttpResponse::Forbidden().finish(),
                        );
                    }

                    proxy::forward(
                        req,
                        body,
                        &upstream_inner.get_ref().0,
                        client_inner.get_ref(),
                    )
                    .await
                }
            }))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}

/// Extracts namespace from `/api/repos/{namespace}/...`.
/// Returns `None` for non-repo paths (/api/version, /api/health, etc.)
/// which are allowed through for any authenticated token.
fn extract_namespace(path: &str) -> Option<&str> {
    path.trim_start_matches('/')
        .strip_prefix("api/repos/")
        .and_then(|rest| rest.split('/').next())
        .filter(|s| !s.is_empty())
}

async fn check_auth(token: Option<String>, namespace: Option<&str>, store: &SharedStore) -> bool {
    let token = match token {
        Some(t) => t,
        None => return false,
    };

    let store_clone = Arc::clone(store);
    let record =
        tokio::task::spawn_blocking(move || store_clone.get_by_token(&token).ok().flatten())
            .await
            .ok()
            .flatten();

    match (record, namespace) {
        (Some(r), Some(ns)) => r.namespaces.iter().any(|n| n == "*" || n == ns),
        (Some(_), None) => true, // non-repo paths: allow any authenticated token
        (None, _) => false,
    }
}
