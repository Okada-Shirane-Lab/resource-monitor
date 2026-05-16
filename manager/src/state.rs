use serde::Serialize;
use shared::SystemMetrics;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone)]
pub struct NetworkConfig {
    pub subnet_prefix: String,
    pub mac_user_csv: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkGradeCount {
    pub grade: String,
    pub online: usize,
    pub offline: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct NetworkUsersSnapshot {
    pub grade_counts: Vec<NetworkGradeCount>,
}

/// マネージャーのアプリケーション状態
pub struct AppState {
    /// マシンIDごとのメトリクス履歴
    metrics: HashMap<String, Vec<SystemMetrics>>,
    /// メトリクスの最大保持数（マシンあたり）
    max_history: usize,
    /// ネットワーク走査設定
    network: NetworkConfig,
    /// ネットワークユーザー情報（キャッシュ）
    network_snapshot: NetworkUsersSnapshot,
}

impl AppState {
    pub fn new(network: NetworkConfig) -> Self {
        Self {
            metrics: HashMap::new(),
            max_history: 288, // 48時間分（10秒間隔で報告された場合）
            network,
            network_snapshot: NetworkUsersSnapshot::default(),
        }
    }

    /// メトリクスを追加
    pub fn add_metrics(&mut self, metrics: SystemMetrics) {
        let machine_id = metrics.machine_id.clone();
        self.metrics
            .entry(machine_id.clone())
            .or_insert_with(Vec::new)
            .push(metrics);

        // 履歴を制限
        if let Some(history) = self.metrics.get_mut(&machine_id) {
            if history.len() > self.max_history {
                history.remove(0);
            }
        }
    }

    /// 最新のメトリクスを取得
    pub fn get_latest_metrics(&self) -> Vec<SystemMetrics> {
        self.metrics
            .values()
            .filter_map(|history| history.last().cloned())
            .collect()
    }

    /// 特定マシンの最新メトリクスを取得
    pub fn get_machine_latest(&self, machine_id: &str) -> Option<SystemMetrics> {
        self.metrics
            .get(machine_id)
            .and_then(|history| history.last().cloned())
    }

    pub fn network_config(&self) -> &NetworkConfig {
        &self.network
    }

    pub fn set_network_snapshot(&mut self, snapshot: NetworkUsersSnapshot) {
        self.network_snapshot = snapshot;
    }

    pub fn network_snapshot(&self) -> NetworkUsersSnapshot {
        self.network_snapshot.clone()
    }
}
