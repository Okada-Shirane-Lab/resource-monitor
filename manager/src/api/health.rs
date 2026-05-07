use axum::Json;
use serde_json::json;

/// ヘルスチェック
pub async fn health_check() -> Json<serde_json::Value> {
    Json(json!({
        "status": "healthy",
        "service": "resource-manager"
    }))
}
