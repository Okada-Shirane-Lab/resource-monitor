# リソースモニター - クイックスタートガイド

## 概要

このプロジェクトは Rust で実装された分散リソース監視システムです。

- **エージェント** (`resource-agent`): 各マシンで動作してリソース情報を収集
- **マネージャー** (`resource-manager`): 複数マシンのメトリクスを集約してWeb UIで表示
- **Shared**: 共通データモデル

## プロジェクト構成

```
resource-monitor/
├── agent/              # エージェントバイナリ
│   ├── Cargo.toml
│   └── src/
│       └── main.rs     (140+ 行)
├── manager/            # マネージャーバイナリ（Web UIを含む）
│   ├── Cargo.toml
│   └── src/
│       ├── api/        # API ハンドラーモジュール
│       │   ├── mod.rs
│       │   ├── metrics.rs
│       │   ├── machines.rs
│       │   ├── health.rs
│       │   └── web.rs  (Web UIダッシュボード)
│       ├── main.rs     (Axumサーバー設定)
│       └── state.rs    (メトリクス状態管理)
├── shared/             # 共有ライブラリ（型定義）
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs      (データモデル定義)
├── Cargo.toml          # ワークスペース設定
└── README.md           # 詳細ドキュメント
```

## インストール

### 前提条件
- Rust 1.70+ (`rustup` でインストール)
- Rocky Linux 8 (または Linux/macOS)

### ビルド

```bash
cd /Users/mary/Documents/resource-monitor

# すべてをビルド
cargo build --release

# バイナリはここに生成される:
# - target/release/resource-agent
# - target/release/resource-manager
```

### GitHub から Manager / Agent をインストール

```bash
# Manager
cargo install --git https://github.com/Okada-Shirane-Lab/resource-monitor manager

# Agent
cargo install --git https://github.com/Okada-Shirane-Lab/resource-monitor agent
```

インストール後は `resource-manager` / `resource-agent` で起動できます。

## 実行方法

### 1️⃣ マネージャーの起動

```bash
./target/release/resource-manager --bind 0.0.0.0 --port 8081
```

Web UI: **http://localhost:8081**

### 2️⃣ エージェントの起動

**別のターミナルで：**

```bash
# 基本：マシンセIDは指定しなければホスト名が使用されます
./target/release/resource-agent --manager-url http://localhost:8081

# カスタマBIDを指定する場合
./target/release/resource-agent \
  --manager-url http://localhost:8081 \
  --machine-id server-01

# 複数マシンの場合
./target/release/resource-agent --manager-url http://192.168.x.x:8081
```

## API エンドポイント

```
GET  /                            # Web UI ダッシュボード
GET  /health                      # ヘルスチェック
POST /api/metrics/report          # メトリクスレポート受信
GET  /api/metrics/latest          # 最新メトリクス取得
GET  /api/machines                # マシン一覧
GET  /api/machines/{machine_id}   # マシン詳細
GET  /api/network/users           # ユーザー在席情報 + 学年ごとの人数
```

## Rocky Linux 8 へのデプロイ

```bash
# 依存関係インストール
sudo dnf install -y curl

# Rustインストール
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# プロジェクトセットアップ
git clone <repo-url> resource-monitor
cd resource-monitor
cargo build --release

# Systemd サービス設定（オプション）
# README.md を参照して /etc/systemd/system/ に .service ファイルを配置
```

## 機能

✅ **複数マシン監視**
- UUID またはカスタムマシンIDで識別
- 複数マシンのメトリクスを一元管理

✅ **リアルタイムダッシュボード**
- CPU、メモリ、ディスク使用率の視覚化
- 10秒ごとの自動更新
- オンライン/オフラインステータス表示

✅ **REST API**
- JSON形式のメトリクス取得
- プログラムからのアクセスが可能

✅ **メトリクス履歴**
- 最大288ポイント保持 (48時間分 @ 10秒間隔)

## トラブルシューティング

### Agent が接続できない
```bash
# ファイアウォール確認
sudo firewall-cmd --permanent --add-port=8081/tcp
sudo firewall-cmd --reload

# マネージャーが稼働中か確認
curl http://localhost:8081/health
```

### ログを確認
```bash
# トレースログを有効にして実行
RUST_LOG=info ./target/release/resource-manager
RUST_LOG=debug ./target/release/resource-agent
```

## 開発

### きれいなビルド
```bash
cargo clean
cargo build --release
```

### テスト（将来実装予定）
```bash
cargo test
```

---

📖 詳細は [README.md](README.md) を参照してください。
