use axum::{extract::State, Json};
use shared::ApiResponse;
use std::collections::{BTreeMap, HashMap, HashSet};
#[cfg(target_os = "macos")]
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::error;

use crate::state::{AppState, NetworkGradeCount, NetworkUsersSnapshot};

pub const NETWORK_SCAN_INTERVAL_SECS: u64 = 10;
const OFFLINE_FAIL_THRESHOLD: u32 = 3;
const ARP_SETTLE_MS: u64 = 800;
const LIVENESS_TIMEOUT_MS: u64 = 600;
const LIVENESS_PORTS: [u16; 8] = [22, 53, 80, 123, 443, 445, 631, 62078];

pub async fn list_users(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Json<ApiResponse<NetworkUsersSnapshot>> {
    let snapshot = {
        let state = state.read().await;
        state.network_snapshot()
    };
    Json(ApiResponse::success(snapshot))
}

pub async fn run_network_collector(state: Arc<RwLock<AppState>>) {
    let mut known_ip_to_user: HashMap<String, CsvUserEntry> = HashMap::new();
    let mut fail_counts: HashMap<String, u32> = HashMap::new();

    loop {
        let (subnet_prefix, csv_path) = {
            let state = state.read().await;
            let network = state.network_config();
            (network.subnet_prefix.clone(), network.mac_user_csv.clone())
        };

        let csv_mapping = match csv_path {
            Some(path) => match load_mac_user_map(&path) {
                Ok(mapping) => mapping,
                Err(e) => {
                    error!("failed to load MAC user CSV: {}", e);
                    HashMap::new()
                }
            },
            None => HashMap::new(),
        };
        let csv_users = build_csv_users(&csv_mapping);

        probe_subnet(&subnet_prefix).await;
        tokio::time::sleep(std::time::Duration::from_millis(ARP_SETTLE_MS)).await;

        let ip_mac_map = match read_arp_ip_mac_map() {
            Ok(map) => map,
            Err(e) => {
                error!("failed to read ARP table: {}", e);
                HashMap::new()
            }
        };
        let arp_ips: HashSet<String> = ip_mac_map
            .keys()
            .filter(|ip| ip.starts_with(&format!("{subnet_prefix}.")))
            .cloned()
            .collect();
        let active_ips = filter_live_ips(arp_ips).await;

        for (ip, mac) in &ip_mac_map {
            if !ip.starts_with(&format!("{subnet_prefix}.")) {
                continue;
            }
            if let Some(user) = csv_mapping.get(&normalize_mac(mac)) {
                known_ip_to_user.insert(ip.clone(), user.clone());
                fail_counts
                    .entry(ip.clone())
                    .or_insert(OFFLINE_FAIL_THRESHOLD);
            }
        }

        for ip in known_ip_to_user.keys() {
            if active_ips.contains(ip) {
                fail_counts.insert(ip.clone(), 0);
            } else {
                let entry = fail_counts
                    .entry(ip.clone())
                    .or_insert(OFFLINE_FAIL_THRESHOLD);
                *entry += 1;
            }
        }

        let presence = build_user_presence(
            &known_ip_to_user,
            &fail_counts,
            &csv_users,
            OFFLINE_FAIL_THRESHOLD,
        );
        let grade_counts = build_grade_counts(&presence);

        {
            let mut state = state.write().await;
            state.set_network_snapshot(NetworkUsersSnapshot {
                grade_counts,
            });
        }

        tokio::time::sleep(std::time::Duration::from_secs(NETWORK_SCAN_INTERVAL_SECS)).await;
    }
}

fn build_user_presence(
    known_ip_to_user: &HashMap<String, CsvUserEntry>,
    fail_counts: &HashMap<String, u32>,
    csv_users: &BTreeMap<String, Option<String>>,
    offline_threshold: u32,
) -> Vec<UserPresence> {
    #[derive(Default)]
    struct UserAgg {
        grade: Option<String>,
        any_online: bool,
    }

    let mut users: BTreeMap<String, UserAgg> = BTreeMap::new();
    for (username, grade) in csv_users {
        users.insert(
            username.clone(),
            UserAgg {
                grade: grade.clone(),
                any_online: false,
            },
        );
    }
    for (ip, user) in known_ip_to_user {
        let agg = users.entry(user.username.clone()).or_insert_with(|| UserAgg {
            grade: user.grade.clone(),
            any_online: false,
        });
        if agg.grade.is_none() && user.grade.is_some() {
            agg.grade = user.grade.clone();
        }

        let fails = fail_counts
            .get(ip)
            .copied()
            .unwrap_or(offline_threshold);
        if fails < offline_threshold {
            agg.any_online = true;
        }
    }

    users
        .into_iter()
        .map(|(_, agg)| UserPresence {
            grade: agg.grade,
            status: if agg.any_online {
                "online".to_string()
            } else {
                "offline".to_string()
            },
        })
        .collect()
}

fn build_grade_counts(users: &[UserPresence]) -> Vec<NetworkGradeCount> {
    let mut counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for user in users {
        let grade = user
            .grade
            .as_deref()
            .map(str::trim)
            .filter(|grade| !grade.is_empty())
            .unwrap_or("未設定")
            .to_string();
        let entry = counts.entry(grade).or_insert((0, 0));
        if user.status == "online" {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }

    counts
        .into_iter()
        .map(|(grade, (online, offline))| NetworkGradeCount {
            grade,
            online,
            offline,
            total: online + offline,
        })
        .collect()
}

#[derive(Debug, Clone)]
struct CsvUserEntry {
    username: String,
    grade: Option<String>,
}

#[derive(Debug, Clone)]
struct UserPresence {
    grade: Option<String>,
    status: String,
}

fn build_csv_users(csv_mapping: &HashMap<String, CsvUserEntry>) -> BTreeMap<String, Option<String>> {
    let mut users: BTreeMap<String, Option<String>> = BTreeMap::new();
    for entry in csv_mapping.values() {
        users
            .entry(entry.username.clone())
            .and_modify(|grade| {
                if grade.is_none() && entry.grade.is_some() {
                    *grade = entry.grade.clone();
                }
            })
            .or_insert_with(|| entry.grade.clone());
    }
    users
}

fn load_mac_user_map(path: &Path) -> anyhow::Result<HashMap<String, CsvUserEntry>> {
    let content = std::fs::read_to_string(path)?;
    let mut map = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let columns: Vec<_> = trimmed.split(',').map(|c| c.trim()).collect();
        if columns.len() < 2 {
            continue;
        }

        let mac = normalize_mac(columns[0]);
        let username = columns[1];
        let grade = columns
            .get(2)
            .map(|grade| grade.trim())
            .filter(|grade| !grade.is_empty());

        if mac == "mac" || username.eq_ignore_ascii_case("username") {
            continue;
        }
        if !mac.is_empty() && !username.is_empty() {
            map.insert(
                mac,
                CsvUserEntry {
                    username: username.to_string(),
                    grade: grade.map(|grade| grade.to_string()),
                },
            );
        }
    }

    Ok(map)
}

async fn probe_subnet(subnet_prefix: &str) {
    let mut tasks = tokio::task::JoinSet::new();
    for host in 2..=254 {
        let ip = format!("{subnet_prefix}.{host}");
        tasks.spawn(async move {
            // UDP送信でARP解決を誘発する
            if let Ok(socket) = tokio::net::UdpSocket::bind("0.0.0.0:0").await {
                let _ = socket.send_to(&[0u8], format!("{ip}:9")).await;
            }
        });
    }
    while tasks.join_next().await.is_some() {
    }
}

async fn filter_live_ips(candidates: HashSet<String>) -> HashSet<String> {
    let mut tasks = tokio::task::JoinSet::new();
    for ip in candidates {
        tasks.spawn(async move {
            let live = is_ip_live(&ip).await;
            (ip, live)
        });
    }

    let mut live_ips = HashSet::new();
    while let Some(res) = tasks.join_next().await {
        if let Ok((ip, true)) = res {
            live_ips.insert(ip);
        }
    }
    live_ips
}

async fn is_ip_live(ip: &str) -> bool {
    for port in LIVENESS_PORTS {
        let target = format!("{ip}:{port}");
        let attempt = tokio::time::timeout(
            std::time::Duration::from_millis(LIVENESS_TIMEOUT_MS),
            tokio::net::TcpStream::connect(&target),
        )
        .await;

        match attempt {
            Ok(Ok(_)) => return true,
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => return true,
            _ => {}
        }
    }
    false
}

#[cfg(target_os = "linux")]
fn read_arp_ip_mac_map() -> anyhow::Result<HashMap<String, String>> {
    let content = std::fs::read_to_string("/proc/net/arp")?;
    let mut map = HashMap::new();

    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }

        let ip = parts[0].to_string();
        let mac = normalize_mac(parts[3]);
        if mac != "00:00:00:00:00:00" {
            map.insert(ip, mac);
        }
    }

    Ok(map)
}

#[cfg(target_os = "macos")]
fn read_arp_ip_mac_map() -> anyhow::Result<HashMap<String, String>> {
    use anyhow::anyhow;
    use std::mem;
    use std::ptr;

    let mut mib = [
        libc::CTL_NET,
        libc::PF_ROUTE,
        0,
        libc::AF_INET,
        libc::NET_RT_FLAGS,
        libc::RTF_LLINFO,
    ];

    let mut needed: usize = 0;
    let res = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            ptr::null_mut(),
            &mut needed,
            ptr::null_mut(),
            0,
        )
    };
    if res != 0 {
        return Err(anyhow!("sysctl size query failed"));
    }
    if needed == 0 {
        return Ok(HashMap::new());
    }

    let mut buf = vec![0u8; needed];
    let res = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            buf.as_mut_ptr() as *mut libc::c_void,
            &mut needed,
            ptr::null_mut(),
            0,
        )
    };
    if res != 0 {
        return Err(anyhow!("sysctl data query failed"));
    }

    let mut map = HashMap::new();
    let mut offset = 0usize;

    while offset + mem::size_of::<libc::rt_msghdr>() <= needed {
        let rtm = unsafe { &*(buf[offset..].as_ptr() as *const libc::rt_msghdr) };
        let msg_len = rtm.rtm_msglen as usize;
        if msg_len == 0 || offset + msg_len > needed {
            break;
        }

        let mut sa_offset = offset + mem::size_of::<libc::rt_msghdr>();
        let mut ip: Option<String> = None;
        let mut mac: Option<String> = None;

        let addrs = rtm.rtm_addrs as usize;
        for i in 0..(libc::RTAX_MAX as usize) {
            if addrs & (1usize << i) == 0 {
                continue;
            }
            if sa_offset >= offset + msg_len {
                break;
            }

            let sa = unsafe { &*(buf[sa_offset..].as_ptr() as *const libc::sockaddr) };
            let sa_len = if sa.sa_len == 0 {
                mem::size_of::<libc::sockaddr>()
            } else {
                sa.sa_len as usize
            };
            if sa_offset + sa_len > offset + msg_len {
                break;
            }

            if i == libc::RTAX_DST as usize && sa.sa_family as i32 == libc::AF_INET {
                let sin = unsafe { &*(buf[sa_offset..].as_ptr() as *const libc::sockaddr_in) };
                let addr = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
                ip = Some(addr.to_string());
            } else if i == libc::RTAX_GATEWAY as usize && sa.sa_family as i32 == libc::AF_LINK {
                let sdl = unsafe { &*(buf[sa_offset..].as_ptr() as *const libc::sockaddr_dl) };
                let nlen = sdl.sdl_nlen as usize;
                let alen = sdl.sdl_alen as usize;
                if alen == 6 {
                    let data_ptr = sdl.sdl_data.as_ptr() as *const u8;
                    let bytes = unsafe { std::slice::from_raw_parts(data_ptr.add(nlen), alen) };
                    mac = Some(format!(
                        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
                    ));
                }
            }

            sa_offset += roundup_to_word(sa_len);
        }

        if let (Some(ip), Some(mac)) = (ip, mac) {
            map.insert(ip, normalize_mac(&mac));
        }

        offset += msg_len;
    }

    Ok(map)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn read_arp_ip_mac_map() -> anyhow::Result<HashMap<String, String>> {
    Ok(HashMap::new())
}

#[cfg(target_os = "macos")]
fn roundup_to_word(len: usize) -> usize {
    let align = std::mem::size_of::<usize>();
    (len + align - 1) & !(align - 1)
}

fn normalize_mac(mac: &str) -> String {
    mac.trim().replace('-', ":").to_lowercase()
}
