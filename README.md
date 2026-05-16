# Resource Monitor

分散型リソースモニタリングシステム。複数のマシンのシステムリソース（CPU、メモリ、ディスク）を一元管理でき、Web UIで可視化します。

## 特徴

- ✅ **Rocky Linux 8 対応**
- 🖥️ **複数マシン監視**: エージェント方式で複数マシンのリソースを監視
- 🌐 **Web ダッシュボード**: リアルタイムのリソース情報を Web UI で表示
- 📊 **メトリクス収集**: CPU、メモリ、ディスク使用率を自動監視
- 🔄 **定期レポート**: 設定可能な間隔でマネージャーにメトリクスを報告

## アーキテクチャ

```
┌─────────────────────────────────────────────────────┐
│                                                     │
│  [マシンA]        [マシンB]        [マシンC]         │
│  ┌──────────┐   ┌──────────┐   ┌──────────┐       │
│  │Klient  │   │  Agent   │   │  Agent   │       │
│  │         │   │  Port:  │   │  Port:  │       │
│  └────┬────┘   └────┬────┘   └────┬────┘       │
│       │             │             │             │
│       └─────────────┴─────────────┘             │
│          ↓ (HTTP POST /api/metrics/report)      │
│  ┌──────────────────────────────┐              │
│  │      Manager                 │              │
│  │   (localhost:8081)           │              │
│  │ ┌────────────────────────┐   │              │
│  │ │ Metrics Storage        │   │              │
│  │ └────────────────────────┘   │              │
│  └──────────────────────────────┘              │
│          ↓ (HTTP GET /api/metrics/latest)      │
│  ┌──────────────────────────────┐              │
│  │   Web UI (Dashboard)         │              │
│  │   http://localhost:8081      │              │
│  └──────────────────────────────┘              │
│                                                     │
└─────────────────────────────────────────────────────┘
```

## コンポーネント

### Agent (resource-agent)
- 各マシンで動作
- CPU、メモリ、ディスク使用率を定期的に収集
- マネージャーにメトリクスを送信

### Manager (resource-manager)
- 中央のメトリクス集約サーバー
- エージェントからのレポートを受け入れる
- Web UI とデータ API を提供

### Shared ライブラリ
- 共通のデータモデルと型定義

## インストール

### 前提条件
- Rust 1.70 以上
- Rocky Linux 8（または Ubuntu, CentOS など）

### ビルド

```bash
# プロジェクト全体をビルド
cargo build --release

# または個別にビルド
cargo build -p agent --release
cargo build -p manager --release
```

### GitHub からインストール（Manager / Agent）

```bash
# Manager
cargo install --git https://github.com/Okada-Shirane-Lab/resource-monitor manager

# Agent
cargo install --git https://github.com/Okada-Shirane-Lab/resource-monitor agent
```

インストール後は `resource-manager` / `resource-agent` コマンドで起動できます。

## 使用方法

### 1. マネージャーの起動

```bash
# デフォルト設定で起動
cargo run -p manager --release -- --bind 0.0.0.0 --port 8081

# または
./target/release/resource-manager --bind 0.0.0.0 --port 8081
```

Web UI: http://localhost:8081

### 2. エージェントの起動（複数マシン）

**マシンA:**
```bash
cargo run -p agent --release -- \
  --manager-url http://manager-server:8081
```

**マシンB:**
```bash
cargo run -p agent --release -- \
  --manager-url http://manager-server:8081
```

### オプション

#### Manager オプション
- `--bind <ADDRESS>`: バインドアドレス（デフォルト: 0.0.0.0）
- `--port <PORT>`: リッスンポート（デフォルト: 8081）
- `--log-level <LEVEL>`: ログレベル（`trace`/`debug`/`info`/`warn`/`error`、デフォルト: `info`）
- `--scan-subnet <PREFIX>`: MAC収集対象サブネット（デフォルト: `172.20.10`）
- `--mac-user-csv <PATH>`: `mac,username,grade` の対応CSVファイルパス
  - Manager がバックグラウンドで定期収集し、Webリクエスト時はキャッシュを返す

#### Agent オプション
- `--manager-url <URL>`: マネージャーサーバーの URL（デフォルト: http://localhost:8081）
- `--interval <SECONDS>`: テレメトリ送信間隔（秒）（デフォルト: 10）
  - Agent は 0.5秒ごとにデータ収集し、指定間隔内の平均値を送信
- `--machine-id <ID>`: マシン識別子（指定しない場合はホスト名を使用）
- `--log-level <LEVEL>`: ログレベル（`trace`/`debug`/`info`/`warn`/`error`、デフォルト: `info`）

### MACとユーザー名CSVの例

```csv
mac,username,grade
aa:bb:cc:dd:ee:ff,alice,M
11:22:33:44:55:66,bob,D
```

Manager 起動例:

```bash
resource-manager \
  --scan-subnet 172.20.10 \
  --mac-user-csv /path/to/mac_users.csv
```

## API エンドポイント

### メトリクス

#### メトリクスレポート受信
```
POST /api/metrics/report
Content-Type: application/json

{
  "metrics": {
    "machine_id": "machine-a",
    "hostname": "server-01",
    "cpu_usage": 45.2,
    "memory_usage": 62.1,
    "memory_used": 4294967296,
    "memory_total": 6912000000,
    "disk_usage": 58.3,
    "disk_used": 1099511627776,
    "disk_total": 1883549859840,
    "timestamp": "2024-04-04T12:34:56Z"
  },
  "agent_version": "0.1.0"
}
```

#### 最新メトリクス取得
```
GET /api/metrics/latest

Response:
{
  "success": true,
  "data": {
    "machines": [
      {
        "machine_id": "machine-a",
        "hostname": "server-01",
        "cpu_usage": 45.2,
        ...
      }
    ],
    "last_updated": "2024-04-04T12:34:56Z"
  }
}
```

### マシン

#### マシン一覧
```
GET /api/machines
```

#### マシン詳細
```
GET /api/machines/{machine_id}
```

#### ネットワークユーザー一覧
```
GET /api/network/users
```

- 返却内容は `data.grade_counts`（学年ごとの在席人数/総人数）です
- MACアドレス一覧はAPIレスポンスに含めません

### ヘルスチェック
```
GET /health
```

## Web UI
http://localhost:8081 にアクセスするとダッシュボードが表示されます。

- 各マシンのリアルタイムメトリクス
- CPU、メモリ、ディスク使用率の視覚化
- オンライン/オフラインステータス
- 自動更新（10秒ごと）

## Rocky Linux 8 へのデプロイ

### 1. 環境準備

```bash
# Rust のインストール
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Git のインストール（必要な場合）
sudo dnf install -y git
```

### 2. プロジェクトのセットアップ

```bash
# プロジェクトのクローンまたはアップロード
git clone https://github.com/yourusername/resource-monitor.git
cd resource-monitor

# ビルド
cargo build --release
```

### 3. Systemd サービス設定（オプション）

**Manager サービス** (`/etc/systemd/system/resource-manager.service`):
```ini
[Unit]
Description=Resource Monitor Manager
After=network.target

[Service]
Type=simple
User=monitor
WorkingDirectory=/opt/resource-monitor
ExecStart=/opt/resource-monitor/resource-manager --bind 0.0.0.0 --port 8081
Restart=on-failure
RestartSec=10s

[Install]
WantedBy=multi-user.target
```

**Agent サービス** (`/etc/systemd/system/resource-agent.service`):
```ini
[Unit]
Description=Resource Monitor Agent
After=network.target

[Service]
Type=simple
User=monitor
WorkingDirectory=/opt/resource-monitor
ExecStart=/opt/resource-monitor/resource-agent --manager-url http://manager-server:8081
Restart=on-failure
RestartSec=10s

[Install]
WantedBy=multi-user.target
```

### 4. サービスの開始

```bash
# サービスの有効化と起動
sudo systemctl enable resource-manager
sudo systemctl start resource-manager

sudo systemctl enable resource-agent
sudo systemctl start resource-agent

# ステータス確認
sudo systemctl status resource-manager
sudo systemctl status resource-agent
```

## トラブルシューティング

### Agent が Manager に接続できない

```bash
# URL の確認
# --manager-url が正しい IP アドレスとポートを指定しているか確認

# ファイアウォール確認
sudo firewall-cmd --list-all
sudo firewall-cmd --permanent --add-port=8081/tcp
sudo firewall-cmd --reload
```

### メトリクスが表示されない

```bash
# ログを確認
# Agent: RUST_LOG=info cargo run -p agent --release
# Manager: RUST_LOG=info cargo run -p manager --release

# ヘルスチェック
curl http://localhost:8081/health

# メトリクス確認
curl http://localhost:8081/api/metrics/latest
```

## 開発

### ログ出力を有効にする
```bash
RUST_LOG=info cargo run -p agent --release
RUST_LOG=info cargo run -p manager --release
```

### テスト
```bash
cargo test
```

## ライセンス
MIT

## サポート
問題が発生した場合は、GitHub Issues で報告してください。
