use axum::response::Html;

/// Web UI のインデックスページを返す
pub async fn index() -> Html<&'static str> {
    Html(
        r#"
<!DOCTYPE html>
<html lang="ja">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Resource Monitor</title>
    <style>
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }

        body {
            font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            padding: 20px;
        }

        .container {
            max-width: 1200px;
            margin: 0 auto;
        }

        h1 {
            color: white;
            margin-bottom: 30px;
            text-align: center;
            font-size: 2.5em;
            text-shadow: 2px 2px 4px rgba(0, 0, 0, 0.3);
        }

        .machines-grid {
            display: grid;
            grid-template-columns: repeat(auto-fill, minmax(350px, 1fr));
            gap: 20px;
            margin-bottom: 20px;
        }

        .machine-card {
            background: white;
            border-radius: 10px;
            padding: 20px;
            box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
            transition: transform 0.2s;
        }

        .machine-card:hover {
            transform: translateY(-5px);
            box-shadow: 0 8px 12px rgba(0, 0, 0, 0.2);
        }

        .machine-header {
            border-bottom: 2px solid #f0f0f0;
            padding-bottom: 10px;
            margin-bottom: 15px;
        }

        .machine-name {
            font-size: 1.3em;
            font-weight: bold;
            color: #333;
        }

        .machine-id {
            font-size: 0.85em;
            color: #999;
            font-family: monospace;
            margin-top: 5px;
        }

        .metric {
            margin-bottom: 15px;
        }

        .metric-label {
            display: flex;
            justify-content: space-between;
            margin-bottom: 5px;
            font-size: 0.9em;
            color: #666;
        }

        .metric-bar {
            width: 100%;
            height: 8px;
            background: #f0f0f0;
            border-radius: 4px;
            overflow: hidden;
        }

        .metric-fill {
            height: 100%;
            border-radius: 4px;
            transition: width 0.3s ease;
        }

        .metric-fill.cpu {
            background: linear-gradient(90deg, #ff6b6b, #ff8c42);
        }

        .metric-fill.memory {
            background: linear-gradient(90deg, #4ecdc4, #44a08d);
        }

        .metric-fill.disk {
            background: linear-gradient(90deg, #a8e6cf, #56ab2f);
        }

        .timestamp {
            font-size: 0.8em;
            color: #999;
            margin-top: 10px;
            border-top: 1px solid #f0f0f0;
            padding-top: 10px;
        }

        .status {
            display: inline-block;
            padding: 5px 10px;
            border-radius: 5px;
            font-size: 0.8em;
            font-weight: bold;
            margin-top: 10px;
        }

        .status.online {
            background: #d4edda;
            color: #155724;
        }

        .status.offline {
            background: #f8d7da;
            color: #721c24;
        }

        .error-message {
            background: #f8d7da;
            color: #721c24;
            padding: 15px;
            border-radius: 5px;
            margin-bottom: 20px;
            text-align: center;
        }

        .loading {
            text-align: center;
            color: white;
            padding: 40px;
            font-size: 1.2em;
        }

        .refresh-info {
            text-align: center;
            color: rgba(255, 255, 255, 0.8);
            margin-top: 20px;
            font-size: 0.9em;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>🖥️ リソースモニター</h1>
        <div id="content" class="loading">データを読み込み中...</div>
        <div class="refresh-info">自動更新: 1秒ごと</div>
    </div>

    <script>
        async function loadMetrics() {
            try {
                const response = await fetch('/api/metrics/latest');
                const data = await response.json();

                if (data.success && data.data && data.data.machines) {
                    renderMetrics(data.data.machines);
                } else {
                    showError('メトリクスデータが利用できません');
                }
            } catch (error) {
                showError('メトリクスの読み込みに失敗しました: ' + error.message);
            }
        }

        function renderMetrics(machines) {
            if (machines.length === 0) {
                document.getElementById('content').innerHTML =
                    '<div class="error-message">接続されたマシンがありません</div>';
                return;
            }

            let html = '<div class="machines-grid">';

            machines.forEach(machine => {
                const isOnline = isRecentlyUpdated(machine.timestamp);
                const statusClass = isOnline ? 'online' : 'offline';
                const statusText = isOnline ? 'オンライン' : 'オフライン';

                html += `
                    <div class="machine-card">
                        <div class="machine-header">
                            <div class="machine-name">${escapeHtml(machine.hostname)}</div>
                            <div class="machine-id">ID: ${escapeHtml(machine.machine_id)}</div>
                            <span class="status ${statusClass}">${statusText}</span>
                        </div>

                        <div class="metric">
                            <div class="metric-label">
                                <span>CPU</span>
                                <span>${machine.cpu_usage.toFixed(1)}%</span>
                            </div>
                            <div class="metric-bar">
                                <div class="metric-fill cpu" style="width: ${Math.min(machine.cpu_usage, 100)}%"></div>
                            </div>
                        </div>

                        <div class="metric">
                            <div class="metric-label">
                                <span>メモリ</span>
                                <span>${machine.memory_usage.toFixed(1)}%</span>
                            </div>
                            <div class="metric-bar">
                                <div class="metric-fill memory" style="width: ${Math.min(machine.memory_usage, 100)}%"></div>
                            </div>
                            <div style="font-size: 0.85em; color: #999; margin-top: 3px;">
                                ${formatBytes(machine.memory_used)} / ${formatBytes(machine.memory_total)}
                            </div>
                        </div>

                        <div style="border-top: 2px solid #f0f0f0; margin-top: 15px; padding-top: 15px;">
                            <div style="font-weight: bold; margin-bottom: 10px; color: #333;">
                                リソース占有ユーザー (上位3名)
                            </div>
                            ${machine.top_users && machine.top_users.length > 0 ? `
                                <div style="font-size: 0.9em;">
                                    ${machine.top_users.map((user, idx) => `
                                        <div style="margin-bottom: 8px; padding: 8px; background: #f9f9f9; border-radius: 4px;">
                                            <div style="font-weight: 500; color: #333;">${idx + 1}. ${escapeHtml(user.username)}</div>
                                            <div style="display: flex; gap: 15px; margin-top: 4px; font-size: 0.85em;">
                                                <span>CPU: <span style="color: #ff6b6b; font-weight: 500;">${user.cpu_percentage.toFixed(1)}%</span></span>
                                                <span>メモリ: <span style="color: #4ecdc4; font-weight: 500;">${user.memory_percentage.toFixed(1)}%</span> (${formatBytes(user.memory_bytes)})</span>
                                            </div>
                                        </div>
                                    `).join('')}
                                </div>
                            ` : '<div style="color: #999; font-size: 0.9em;">ユーザー情報なし</div>'}
                        </div>

                        <div class="timestamp">
                            更新: ${new Date(machine.timestamp).toLocaleString('ja-JP')}
                        </div>
                    </div>
                `;
            });

            html += '</div>';
            document.getElementById('content').innerHTML = html;
        }

        function showError(message) {
            document.getElementById('content').innerHTML =
                `<div class="error-message">${escapeHtml(message)}</div>`;
        }

        function isRecentlyUpdated(timestamp) {
            const updated = new Date(timestamp);
            const now = new Date();
            const diffSeconds = (now - updated) / 1000;
            return diffSeconds < 60; // 60秒以内
        }

        function formatBytes(bytes) {
            if (bytes === 0) return '0 B';
            const k = 1024;
            const sizes = ['B', 'KB', 'MB', 'GB'];
            const i = Math.floor(Math.log(bytes) / Math.log(k));
            return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
        }

        function escapeHtml(text) {
            const div = document.createElement('div');
            div.textContent = text;
            return div.innerHTML;
        }

        // 初期読み込み
        loadMetrics();

        // 1秒ごとに更新
        setInterval(loadMetrics, 1000);
    </script>
</body>
</html>
    "#,
    )
}
