mod api;
mod state;

use anyhow::Result;
use axum::{
    routing::{get, post},
    Router,
};
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::info;

use state::AppState;

#[derive(Parser, Debug)]
#[command(author, version, about = "Resource Monitor Manager")]
struct Args {
    /// バインドアドレス
    #[arg(short, long, default_value = "0.0.0.0")]
    bind: String,

    /// リッスンポート
    #[arg(short, long, default_value = "8081")]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    info!("Resource Monitor Manager starting...");

    // アプリケーション状態を初期化
    let state = Arc::new(RwLock::new(AppState::new()));

    // ルートを設定
    let app = Router::new()
        // メトリクスAPI
        .route("/api/metrics/report", post(api::metrics::report))
        .route("/api/metrics/latest", get(api::metrics::get_latest))
        .route("/api/machines", get(api::machines::list_machines))
        .route("/api/machines/:machine_id", get(api::machines::get_machine))
        // ヘルスチェック
        .route("/health", get(api::health::health_check))
        // Web UI
        .route("/", get(api::web::index))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", args.bind, args.port).parse()?;
    info!("Manager listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
