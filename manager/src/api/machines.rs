use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use shared::{ApiResponse, SystemMetrics};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::state::AppState;

/// 登録されているマシンの一覧を取得
pub async fn list_machines(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Json<ApiResponse<Vec<SystemMetrics>>> {
    let state = state.read().await;
    let machines = state.get_latest_metrics();
    Json(ApiResponse::success(machines))
}

/// 特定マシンの詳細情報を取得
pub async fn get_machine(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(machine_id): Path<String>,
) -> Result<Json<ApiResponse<SystemMetrics>>, (StatusCode, Json<ApiResponse<()>>)> {
    let state = state.read().await;
    match state.get_machine_latest(&machine_id) {
        Some(metrics) => Ok(Json(ApiResponse::success(metrics))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error("Machine not found")),
        )),
    }
}
