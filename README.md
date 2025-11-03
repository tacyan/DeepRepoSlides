# DeepRepoSlides MCP

Rust製のMCPツールで、ローカル/Mono-Repoを静的解析して日本語要約を作成し、DeepWiki風のドキュメントサイトとスライドを自動生成してGitHub Pagesで公開できるようにします。

## 機能

- **多言語対応の静的解析**: TypeScript/JavaScript, Python, Go, Rust, Javaなど
- **日本語要約生成**: LLMまたは静的ヒューリスティックによる要約
- **DeepWiki風ドキュメント生成**: mdBookベースのWikiサイト（Mermaid対応）
- **スライド生成**: mdbook-revealまたはMarpによるスライド生成
- **GitHub Pages連携**: docs/またはgh-pagesブランチへの自動公開
- **16並列実行**: swarm-mcp-liteを使用した並列処理対応

## セットアップ

```bash
# プロジェクトをビルド
cargo build --release
```

## 使用方法

### 🚀 クイックスタート: このリポジトリ自体を16並列で改善

Cursor内で以下のコマンドを実行してください：

```bash
# 1. プロジェクトをビルド
cargo build --release

# 2. 設定ファイルを作成（初回のみ）
cat > deeprepo.toml << 'EOF'
[project]
name = "DeepRepoSlides"
repo-path = "."
include = ["**/*.rs", "**/*.toml", "**/*.md"]
exclude = ["**/target/**", "**/.git/**", "**/node_modules/**"]

[analysis]
languages = ["rs"]
max-file-kb = 512

[analysis.diagrams]
types = ["module-graph", "call-graph"]
renderer = "mermaid"

[summarization]
mode = "auto"
style = "concise-ja"

[site]
flavor = "mdbook"
out-dir = "./out/wiki"

[slides]
flavor = "mdbook-reveal"
out-dir = "./out/slides"

[publish]
mode = "docs"
branch = "gh-pages"
EOF

# 3. このリポジトリをインデックス化してWikiを生成（16並列対応）
./target/release/deeprepo-slides-mcp build-all -c deeprepo.toml

# 4. 生成されたWikiを確認
open ./out/wiki/book/index.html  # macOS
# または
xdg-open ./out/wiki/book/index.html  # Linux
```

**16並列実行について:**
- Wiki生成機能は自動的に各セクションを並列実行します
- 複数のセクション（overview, architecture, modules, flows, deploy, faq）が同時に生成されます
- パフォーマンスが大幅に向上します

### 方法1: swarm-mcp-liteで16並列実行（推奨）

```bash
# 1. swarm-mcp-liteのセッションを起動（16ペイン）
swarm-mcp-lite swarm

# 2. 16個のペインでMCPサーバーを起動
for i in {0..15}; do
  tmux send-keys -t swarm-multiagent:0.$i "cd $(pwd)" C-m
  tmux send-keys -t swarm-multiagent:0.$i "export RUN_AS_MCP=1" C-m
  tmux send-keys -t swarm-multiagent:0.$i "./target/release/deeprepo-slides-mcp" C-m
  sleep 0.1
done

# 3. このリポジトリをインデックス化
./target/release/deeprepo-slides-mcp index --repo . -c deeprepo.toml

# 4. Wikiを生成
./target/release/deeprepo-slides-mcp wiki --out ./out/wiki -c deeprepo.toml

# 5. スライドを生成
./target/release/deeprepo-slides-mcp slides --flavor mdbook-reveal --out ./out/slides -c deeprepo.toml

# 6. 全機能を一度にビルド（推奨）
./target/release/deeprepo-slides-mcp build-all -c deeprepo.toml
```

### 方法2: CLIとして単一実行

```bash
# リポジトリをインデックス化
./target/release/deeprepo-slides-mcp index --repo . -c deeprepo.toml

# 要約を生成
./target/release/deeprepo-slides-mcp summarize --scope repo --target . --style concise-ja

# Wikiを生成
./target/release/deeprepo-slides-mcp wiki --out ./out/wiki -c deeprepo.toml

# スライドを生成
./target/release/deeprepo-slides-mcp slides \
  --flavor mdbook-reveal \
  --out ./out/slides \
  --sections "overview,architecture,modules" \
  --export "html" \
  -c deeprepo.toml

# GitHub Pagesに公開
./target/release/deeprepo-slides-mcp publish \
  --mode docs \
  --site_dir ./out/wiki \
  --slides_dir ./out/slides \
  --repo_root . \
  --branch gh-pages

# 全機能を一度にビルド（推奨）
./target/release/deeprepo-slides-mcp build-all -c deeprepo.toml
```

### 方法3: MCPサーバーとして実行

```bash
# 単一のMCPサーバーを起動
export RUN_AS_MCP=1
./target/release/deeprepo-slides-mcp

# または開発時
export RUN_AS_MCP=1
cargo run --release
```

### 方法4: システムにインストール

```bash
# インストール（デフォルトでは ~/.cargo/bin にインストールされます）
cargo install --path .

# インストール後はどこからでも実行可能
deeprepo-slides-mcp index --repo . -c deeprepo.toml
```

## 実際の使用例

### このリポジトリ自体を解析・改善する

```bash
# 1. 設定ファイルを作成（初回のみ）
cp deeprepo.toml.example deeprepo.toml
# deeprepo.tomlを編集して、このリポジトリのパスを設定

# 2. 16並列で改善を実行
# swarm-mcp-liteセッションを起動
swarm-mcp-lite swarm

# MCPサーバーを16並列で起動
for i in {0..15}; do
  tmux send-keys -t swarm-multiagent:0.$i "cd $(pwd) && export RUN_AS_MCP=1 && ./target/release/deeprepo-slides-mcp" C-m
  sleep 0.1
done

# リポジトリを解析・改善
./target/release/deeprepo-slides-mcp build-all -c deeprepo.toml
```

### 他のリポジトリを解析する

```bash
# 1. 設定ファイルを作成
cp deeprepo.toml.example deeprepo.toml
# deeprepo.tomlのrepo_pathを変更

# 2. インデックス化
./target/release/deeprepo-slides-mcp index --repo /path/to/your/repo -c deeprepo.toml

# 3. Wikiとスライドを生成
./target/release/deeprepo-slides-mcp build-all -c deeprepo.toml
```

## 設定ファイル

`deeprepo.toml`をプロジェクトルートに配置してください。例：

```bash
cp deeprepo.toml.example deeprepo.toml
```

設定ファイルの主な項目：
- `repo_path`: 解析するリポジトリのパス（デフォルト: "."）
- `include`: 含めるファイルパターン
- `exclude`: 除外するファイルパターン
- `out_dir`: 出力ディレクトリ

詳細は`deeprepo.toml.example`を参照してください。

## 16並列実行の確認

```bash
# 実行中のMCPサーバーの数を確認
ps aux | grep "deeprepo-slides-mcp" | grep -v grep | wc -l

# tmuxペインの状態を確認
tmux list-panes -t swarm-multiagent:0 -F "#{pane_index}: #{pane_current_command}"

# MCPサーバーを停止
pkill -f deeprepo-slides-mcp
```

## ライセンス

MIT OR Apache-2.0
