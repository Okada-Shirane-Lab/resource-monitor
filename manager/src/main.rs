mod api;
mod state;

use anyhow::Result;
use axum::{
    routing::{get, post},
    Router,
};
use clap::{Parser, ValueEnum};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::info;
use tracing::level_filters::LevelFilter;

use state::AppState;

#[derive(Copy, Clone, Debug, ValueEnum)]
enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for LevelFilter {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Trace => LevelFilter::TRACE,
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Warn => LevelFilter::WARN,
            LogLevel::Error => LevelFilter::ERROR,
        }
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Resource Monitor Manager")]
struct Args {
    /// バインドアドレス
    #[arg(short, long, default_value = "0.0.0.0")]
    bind: String,

    /// リッスンポート
    #[arg(short, long, default_value = "8081")]
    port: u16,

    /// ログレベル
    #[arg(long, value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,

    /// 走査対象サブネット（末尾オクテットを除いたプレフィックス）
    #[arg(long, default_value = "172.20.10")]
    scan_subnet: String,

    /// MACアドレスとユーザー名の対応CSVファイル
    #[arg(long)]
    mac_user_csv: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::from(args.log_level))
        .init();
    info!("Resource Monitor Manager starting...");

    // アプリケーション状態を初期化
    let state = Arc::new(RwLock::new(AppState::new(state::NetworkConfig {
        subnet_prefix: args.scan_subnet.clone(),
        mac_user_csv: args.mac_user_csv.clone(),
    })));
    let network_state = state.clone();
    tokio::spawn(async move {
        api::network::run_network_collector(network_state).await;
    });
    let static_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");

    // ルートを設定
    let app = Router::new()
        // メトリクスAPI
        .route("/api/metrics/report", post(api::metrics::report))
        .route("/api/metrics/latest", get(api::metrics::get_latest))
        .route("/api/machines", get(api::machines::list_machines))
        .route("/api/machines/:machine_id", get(api::machines::get_machine))
        .route("/api/network/users", get(api::network::list_users))
        // ヘルスチェック
        .route("/health", get(api::health::health_check))
        // Web UI の静的ファイル
        .fallback_service(ServeDir::new(static_dir))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", args.bind, args.port).parse()?;
    info!("Manager listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
