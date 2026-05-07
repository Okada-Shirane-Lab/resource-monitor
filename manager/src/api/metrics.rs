use axum::{
    extract::{State, Json},
    http::StatusCode,
};
use shared::{MetricsReport, ApiResponse, AggregatedMetrics};
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::Utc;
use tracing::info;

use crate::state::AppState;

/// メトリクスレポートを受け付ける
pub async fn report(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(report): Json<MetricsReport>,
) -> (StatusCode, Json<ApiResponse<String>>) {
    info!(
        "Received metrics from {}: CPU={:.1}%, Memory={:.1}%",
        report.metrics.hostname, report.metrics.cpu_usage, report.metrics.memory_usage
    );

    let mut state = state.write().await;
    state.add_metrics(report.metrics);

    (
        StatusCode::OK,
        Json(ApiResponse::success("Metrics received".to_string())),
    )
}

/// 最新のメトリクスを取得
pub async fn get_latest(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Json<ApiResponse<AggregatedMetrics>> {
    let state = state.read().await;
    let machines = state.get_latest_metrics();

    Json(ApiResponse::success(AggregatedMetrics {
        machines,
        last_updated: Utc::now(),
    }))
}
