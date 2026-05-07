use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// ユーザーのリソース使用情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserResourceUsage {
    /// ユーザー名
    pub username: String,
    /// CPU使用率（0-100）
    pub cpu_percentage: f64,
    /// メモリ使用率（0-100）
    pub memory_percentage: f64,
    /// 使用中のメモリ（バイト）
    pub memory_bytes: u64,
}

/// エージェントから報告されるシステムリソース情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    /// マシンの一意識別子
    pub machine_id: String,
    /// マシンのホスト名
    pub hostname: String,
    /// CPU使用率（0-100）
    pub cpu_usage: f64,
    /// メモリ使用率（0-100）
    pub memory_usage: f64,
    /// 使用中のメモリ（バイト）
    pub memory_used: u64,
    /// 合計メモリ（バイト）
    pub memory_total: u64,
    /// リソース占有ユーザー上位3名
    pub top_users: Vec<UserResourceUsage>,
    /// レポート時刻
    pub timestamp: DateTime<Utc>,
}

/// エージェントからマネージャーへの報告リクエスト
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsReport {
    pub metrics: SystemMetrics,
    pub agent_version: String,
}

/// マネージャーが保持する複数マシンのメトリクス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedMetrics {
    pub machines: Vec<SystemMetrics>,
    pub last_updated: DateTime<Utc>,
}

/// API応答の標準フォーマット
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: Utc::now(),
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
            timestamp: Utc::now(),
        }
    }
}

/// エラー型
#[derive(Debug, thiserror::Error)]
pub enum ResourceMonitorError {
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("System error: {0}")]
    SystemError(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

pub type Result<T> = std::result::Result<T, ResourceMonitorError>;
