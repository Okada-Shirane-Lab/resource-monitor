use anyhow::Result;
use chrono::Utc;
use clap::{Parser, ValueEnum};
use shared::{MetricsReport, SystemMetrics, UserResourceUsage};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, error, info};
use tracing::level_filters::LevelFilter;
use uuid::Uuid;

const SAMPLE_INTERVAL_MS: u64 = 500;
const DEFAULT_REPORT_INTERVAL_SECS: u64 = 10;

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
#[command(author, version, about = "Resource Monitor Agent")]
struct Args {
    /// マネージャーサーバーのURL
    #[arg(short, long, default_value = "http://localhost:8081")]
    manager_url: String,

    /// テレメトリ送信間隔（秒）
    #[arg(short, long, default_value_t = DEFAULT_REPORT_INTERVAL_SECS)]
    interval: u64,

    /// マシンID（指定しない場合はホスト名を使用）
    #[arg(short, long)]
    machine_id: Option<String>,

    /// ログレベル
    #[arg(long, value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

#[derive(Default)]
struct UserAggregation {
    cpu_sum: f64,
    memory_bytes_sum: u64,
}

#[derive(Default)]
struct MetricsAccumulator {
    sample_count: u64,
    cpu_usage_sum: f64,
    memory_usage_sum: f64,
    memory_used_sum: u64,
    memory_total_last: u64,
    users: HashMap<String, UserAggregation>,
}

impl MetricsAccumulator {
    fn add_sample(&mut self, metrics: &SystemMetrics) {
        self.sample_count += 1;
        self.cpu_usage_sum += metrics.cpu_usage;
        self.memory_usage_sum += metrics.memory_usage;
        self.memory_used_sum += metrics.memory_used;
        self.memory_total_last = metrics.memory_total;

        for user in &metrics.top_users {
            let entry = self.users.entry(user.username.clone()).or_default();
            entry.cpu_sum += user.cpu_percentage;
            entry.memory_bytes_sum += user.memory_bytes;
        }
    }

    fn build_averaged_metrics(&self, machine_id: &str, hostname: &str) -> Option<SystemMetrics> {
        if self.sample_count == 0 {
            return None;
        }

        let denominator = self.sample_count as f64;
        let memory_total = self.memory_total_last;
        let mut top_users: Vec<UserResourceUsage> = self
            .users
            .iter()
            .map(|(username, agg)| {
                let cpu_percentage = agg.cpu_sum / denominator;
                let memory_bytes = (agg.memory_bytes_sum as f64 / denominator).round() as u64;
                let memory_percentage = if memory_total > 0 {
                    ((memory_bytes as f64 / memory_total as f64) * 100.0).min(100.0)
                } else {
                    0.0
                };

                UserResourceUsage {
                    username: username.clone(),
                    cpu_percentage,
                    memory_percentage,
                    memory_bytes,
                }
            })
            .collect();

        top_users.sort_by(|a, b| {
            b.cpu_percentage
                .partial_cmp(&a.cpu_percentage)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        top_users.truncate(3);

        Some(SystemMetrics {
            machine_id: machine_id.to_string(),
            hostname: hostname.to_string(),
            cpu_usage: self.cpu_usage_sum / denominator,
            memory_usage: self.memory_usage_sum / denominator,
            memory_used: (self.memory_used_sum as f64 / denominator).round() as u64,
            memory_total,
            top_users,
            timestamp: Utc::now(),
        })
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::from(args.log_level))
        .init();
    info!("Resource Monitor Agent starting...");

    let hostname = hostname::get()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // マシンIDは指定されない場合はホスト名を使用、さらに失敗時はUUID
    let machine_id = args.machine_id.unwrap_or_else(|| {
        if !hostname.is_empty() {
            hostname.clone()
        } else {
            Uuid::new_v4().to_string()
        }
    });

    info!(
        "Machine ID: {}, Hostname: {}, Manager URL: {}",
        machine_id, hostname, args.manager_url
    );

    let client = reqwest::Client::new();
    let report_interval = Duration::from_secs(args.interval);
    let sample_interval = Duration::from_millis(SAMPLE_INTERVAL_MS);
    info!(
        "Sampling every {}ms, reporting averaged metrics every {}s",
        SAMPLE_INTERVAL_MS, args.interval
    );

    // System インスタンスを事前に初期化してCPU測定用の基準値を作成
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();
    tokio::time::sleep(Duration::from_millis(100)).await;
    sys.refresh_all();

    let mut sample_ticker = tokio::time::interval(sample_interval);
    let mut report_started_at = tokio::time::Instant::now();
    let mut accumulator = MetricsAccumulator::default();

    loop {
        sample_ticker.tick().await;

        match collect_metrics(&machine_id, &hostname, &mut sys) {
            Ok(metrics) => {
                let sampled_top_users = metrics
                    .top_users
                    .iter()
                    .map(|user| {
                        format!(
                            "{}: cpu={:.1}%, mem={:.1}%({:.2}GB)",
                            user.username,
                            user.cpu_percentage,
                            user.memory_percentage,
                            user.memory_bytes as f64 / 1024.0 / 1024.0 / 1024.0
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                debug!(
                    "Sampled metrics - machine={}, cpu={:.1}%, mem={:.1}% ({:.2}GB/{:.2}GB), top_users=[{}]",
                    metrics.machine_id,
                    metrics.cpu_usage,
                    metrics.memory_usage,
                    metrics.memory_used as f64 / 1024.0 / 1024.0 / 1024.0,
                    metrics.memory_total as f64 / 1024.0 / 1024.0 / 1024.0,
                    sampled_top_users
                );
                accumulator.add_sample(&metrics);
            }
            Err(e) => {
                error!("Failed to collect metrics: {:?}", e);
            }
        }

        if report_started_at.elapsed() >= report_interval {
            if let Some(metrics) = accumulator.build_averaged_metrics(&machine_id, &hostname) {
                let report = MetricsReport {
                    metrics: metrics.clone(),
                    agent_version: env!("CARGO_PKG_VERSION").to_string(),
                };

                let url = format!("{}/api/metrics/report", &args.manager_url);
                match client.post(&url).json(&report).send().await {
                    Ok(response) => {
                        if response.status().is_success() {
                            info!(
                                "Averaged metrics sent - CPU: {:.1}%, Memory: {:.1}%",
                                metrics.cpu_usage, metrics.memory_usage
                            );
                        } else {
                            error!("Manager returned error: {}", response.status());
                        }
                    }
                    Err(e) => {
                        error!("Failed to send metrics: {:?}", e);
                    }
                }
            } else {
                error!("No samples collected during report window");
            }

            accumulator.reset();
            report_started_at = tokio::time::Instant::now();
        }
    }
}

/// システムメトリクスを収集
fn collect_metrics(
    machine_id: &str,
    hostname: &str,
    sys: &mut sysinfo::System,
) -> Result<SystemMetrics> {
    // 毎回 refresh してCPU使用率を更新
    sys.refresh_all();

    let cpu_usage = sys.global_cpu_info().cpu_usage() as f64;
    let memory_total = sys.total_memory();
    let memory_used = sys.used_memory();
    let memory_usage = if memory_total > 0 {
        (memory_used as f64 / memory_total as f64) * 100.0
    } else {
        0.0
    };

    // ユーザーごとのリソース使用を計算
    let top_users = calculate_user_resources(sys, memory_total, memory_used, cpu_usage);

    Ok(SystemMetrics {
        machine_id: machine_id.to_string(),
        hostname: hostname.to_string(),
        cpu_usage,
        memory_usage,
        memory_used,
        memory_total,
        top_users,
        timestamp: Utc::now(),
    })
}

/// UID からユーザー名を取得（libc を使用）
fn get_username_for_uid(uid: &sysinfo::Uid) -> String {
    // Uid のデバッグ表示から数値を抽出
    let uid_str = format!("{:?}", uid);
    let uid_num: u32 = if uid_str.starts_with("Uid(") && uid_str.ends_with(")") {
        uid_str[4..uid_str.len() - 1].parse().unwrap_or(u32::MAX)
    } else {
        return uid_str;
    };

    // libc を使ってユーザー情報を取得
    let mut passwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    let bufsize = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    let bufsize = if bufsize < 0 { 1024 } else { bufsize as usize };
    let mut buf = vec![0u8; bufsize];

    let ret = unsafe {
        libc::getpwuid_r(
            uid_num as libc::uid_t,
            &mut passwd,
            buf.as_mut_ptr() as *mut i8,
            bufsize,
            &mut result,
        )
    };

    if ret == 0 && !result.is_null() {
        if let Ok(name) = unsafe { std::ffi::CStr::from_ptr(passwd.pw_name).to_str() } {
            name.to_string()
        } else {
            uid_num.to_string()
        }
    } else {
        uid_num.to_string()
    }
}

/// ユーザーごとのリソース使用を計算
fn calculate_user_resources(
    sys: &sysinfo::System,
    total_memory: u64,
    memory_used: u64,
    machine_cpu_usage: f64,
) -> Vec<UserResourceUsage> {
    let mut user_resources: HashMap<String, (f64, u64)> = HashMap::new();
    let mut total_process_cpu = 0.0f64;

    // プロセスごとにユーザー単位で集計
    for process in sys.processes().values() {
        if let Some(user_id) = process.user_id() {
            // UID からユーザー名を取得
            let username = get_username_for_uid(user_id);

            let cpu_percent = process.cpu_usage() as f64;
            // sysinfo 0.30 の process.memory() はバイト単位
            let mem_bytes = process.memory();

            let entry = user_resources.entry(username).or_insert((0.0, 0));
            entry.0 += cpu_percent;
            entry.1 += mem_bytes;
            total_process_cpu += cpu_percent;
        }
    }

    let total_user_memory: u64 = user_resources.values().map(|(_, mem)| *mem).sum();

    // ユーザーを CPU 使用率でソート（CPU使用量だけで判定）
    let mut users: Vec<_> = user_resources
        .into_iter()
        .map(|(username, (cpu, mem))| {
            // 各ユーザーの process CPU 合算を、全 process 合算に対する比率で
            // マシン全体CPU使用率へ配分する（総和が machine_cpu_usage に近づく）
            let cpu_percentage = if total_process_cpu > 0.0 {
                ((cpu / total_process_cpu) * machine_cpu_usage).min(100.0)
            } else {
                0.0
            };

            // ユーザーごとの生メモリ値を割合化し、マシン実使用メモリへ配分して整合性を保つ
            let memory_bytes = if total_user_memory > 0 {
                ((mem as f64 / total_user_memory as f64) * memory_used as f64).round() as u64
            } else {
                0
            };

            let memory_percentage = if total_memory > 0 {
                ((memory_bytes as f64 / total_memory as f64) * 100.0).min(100.0)
            } else {
                0.0
            };

            UserResourceUsage {
                username,
                cpu_percentage,
                memory_percentage,
                memory_bytes,
            }
        })
        .collect();

    // CPU使用率でソートして上位3つを取得
    users.sort_by(|a, b| {
        b.cpu_percentage
            .partial_cmp(&a.cpu_percentage)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    users.into_iter().take(3).collect()
}
