use shared::SystemMetrics;
use std::collections::HashMap;
use chrono::{Utc, Duration};

/// マネージャーのアプリケーション状態
pub struct AppState {
    /// マシンIDごとのメトリクス履歴
    metrics: HashMap<String, Vec<SystemMetrics>>,
    /// メトリクスの最大保持数（マシンあたり）
    max_history: usize,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            metrics: HashMap::new(),
            max_history: 288, // 48時間分（10秒間隔で報告された場合）
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

    /// 特定マシンのメトリクス履歴を取得
    pub fn get_machine_history(&self, machine_id: &str, minutes: i64) -> Vec<SystemMetrics> {
        let cutoff = Utc::now() - Duration::minutes(minutes);
        self.metrics
            .get(machine_id)
            .map(|history| {
                history
                    .iter()
                    .filter(|m| m.timestamp > cutoff)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 登録されているマシンのリスト
    pub fn get_machine_ids(&self) -> Vec<String> {
        self.metrics.keys().cloned().collect()
    }
}
