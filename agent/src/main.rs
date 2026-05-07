use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use shared::{MetricsReport, SystemMetrics, UserResourceUsage};
use std::time::Duration;
use sysinfo::System;
use tracing::{error, info};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(author, version, about = "Resource Monitor Agent")]
struct Args {
    /// マネージャーサーバーのURL
    #[arg(short, long, default_value = "http://localhost:8081")]
    manager_url: String,

    /// リポート間隔（秒）
    #[arg(short, long, default_value = "1")]
    interval: u64,

    /// マシンID（指定しない場合はホスト名を使用）
    #[arg(short, long)]
    machine_id: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // ログを初期化
    tracing_subscriber::fmt::init();

    let args = Args::parse();
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
    let interval = Duration::from_secs(args.interval);

    // System インスタンスを事前に初期化してCPU測定用の基準値を作成
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();
    tokio::time::sleep(Duration::from_millis(100)).await;
    sys.refresh_all();

    loop {
        match collect_metrics(&machine_id, &hostname, &mut sys) {
            Ok(metrics) => {
                let report = MetricsReport {
                    metrics: metrics.clone(),
                    agent_version: env!("CARGO_PKG_VERSION").to_string(),
                };

                // メトリクスをマネージャーに送信
                let url = format!("{}/api/metrics/report", &args.manager_url);
                match client.post(&url).json(&report).send().await {
                    Ok(response) => {
                        if response.status().is_success() {
                            info!(
                                "Metrics sent successfully - CPU: {:.1}%, Memory: {:.1}%",
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
            }
            Err(e) => {
                error!("Failed to collect metrics: {:?}", e);
            }
        }

        tokio::time::sleep(interval).await;
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
    let top_users = calculate_user_resources(sys, memory_total, cpu_usage);

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
    _total_cpu_usage: f64,
) -> Vec<UserResourceUsage> {
    use std::collections::HashMap;

    let mut user_resources: HashMap<String, (f64, u64)> = HashMap::new();

    // プロセスごとにユーザー単位で集計
    for process in sys.processes().values() {
        if let Some(user_id) = process.user_id() {
            // UID からユーザー名を取得
            let username = get_username_for_uid(user_id);

            let cpu_percent = process.cpu_usage() as f64;
            // process.memory() は KiB（キビバイト）で返される
            let mem_bytes = process.memory() * 1024;

            let entry = user_resources.entry(username).or_insert((0.0, 0));
            entry.0 += cpu_percent;
            entry.1 += mem_bytes;
        }
    }

    // ユーザーを CPU 使用率でソート（CPU使用量だけで判定）
    let mut users: Vec<_> = user_resources
        .into_iter()
        .map(|(username, (cpu, mem))| {
            // CPU使用率：process.cpu_usage()はパーセンテージで返される
            let cpu_percentage = cpu.min(100.0);
            // メモリ使用率：mem は既にバイト単位、total_memory も KiB なので KiB に合わせる
            let memory_percentage = if total_memory > 0 {
                let mem_kib = mem / 1024;
                ((mem_kib as f64 / total_memory as f64) * 100.0).min(100.0)
            } else {
                0.0
            };

            UserResourceUsage {
                username,
                cpu_percentage,
                memory_percentage,
                memory_bytes: mem,
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

/// ディスク使用量を取得
#[cfg(target_os = "macos")]
fn get_disk_usage() -> Result<(u64, u64)> {
    use std::process::Command;
    let output = Command::new("df").arg("-k").arg("/").output()?;
    let stdout = String::from_utf8(output.stdout)?;
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() > 1 {
        let parts: Vec<&str> = lines[1].split_whitespace().collect();
        if parts.len() >= 3 {
            let total = parts[1].parse::<u64>().unwrap_or(0) * 1024;
            let used = parts[2].parse::<u64>().unwrap_or(0) * 1024;
            return Ok((total, used));
        }
    }
    Ok((0, 0))
}

#[cfg(not(target_os = "macos"))]
/// メトリクスをマネージャーに送信
async fn send_report(
    client: &reqwest::Client,
    manager_url: &str,
    report: &MetricsReport,
) -> Result<()> {
    let url = format!("{}/api/metrics/report", manager_url);
    let response = client
        .post(&url)
        .json(report)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send report: {}", e))?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Manager returned error: {}",
            response.status()
        ));
    }

    Ok(())
}
