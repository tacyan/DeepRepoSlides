# DeepRepoSlides MCP

Rust製のMCPツールで、ローカル/Mono-Repoを静的解析して日本語要約を作成し、DeepWiki風のドキュメントサイトとスライドを自動生成してGitHub Pagesで公開できるようにします。

## 機能

- **多言語対応の静的解析**: TypeScript/JavaScript, Python, Go, Rust, Javaなど
- **日本語要約生成**: LLMまたは静的ヒューリスティックによる要約
- **16並列処理**: tech-book-readerの50並列翻訳を参考に、モジュール説明を16並列で日本語化
- **1ページ1センテンス形式**: スライドを1ページ1センテンス形式で生成
- **DeepWiki風ドキュメント生成**: mdBookベースのWikiサイト（Mermaid対応）
- **スライド生成**: mdbook-revealまたはMarpによるスライド生成
- **GitHub Pages連携**: docs/またはgh-pagesブランチへの自動公開
- **MCPサーバー対応**: Model Context Protocol (MCP) サーバーとして使用可能

## MCPサーバーとしての使用

### 設定方法

CursorやClaude DesktopなどのMCPクライアントで使用する場合、以下の設定を追加してください：

```json
{
  "mcpServers": {
    "deeprepo-slides": {
      "command": "/path/to/deeprepo-slides-mcp",
      "env": {
        "RUN_AS_MCP": "1"
      }
    }
  }
}
```

### 利用可能なツール

- `index_repo`: リポジトリをインデックス化
- `summarize`: コードの要約を生成
- `generate_wiki`: Wikiサイトを生成
- `generate_slides`: スライドを生成（16並列処理で日本語化）
- `publish_pages`: GitHub Pagesに公開
- `search`: コードベースを検索

## セットアップ

```bash
# プロジェクトをビルド
cargo build --release
```

## 使用方法

### 🚀 クイックスタート: このリポジトリ自体を16並列で改善

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
```

**16並列処理について:**
- Wiki生成機能は自動的に各セクションを並列実行します
- スライド生成では、モジュール説明を16並列で日本語化処理します
- tech-book-readerの50並列翻訳機能を参考に実装されています
- 1ページ1センテンス形式でスライドを生成します

### 方法1: MCPサーバーとして実行

```bash
# MCPサーバーとして起動
export RUN_AS_MCP=1
./target/release/deeprepo-slides-mcp
```

### 方法2: CLIとして単一実行

```bash
# リポジトリをインデックス化
./target/release/deeprepo-slides-mcp index --repo . -c deeprepo.toml

# 要約を生成
./target/release/deeprepo-slides-mcp summarize --scope repo --target . --style concise-ja

# Wikiを生成
./target/release/deeprepo-slides-mcp wiki --out ./out/wiki -c deeprepo.toml

# スライドを生成（16並列処理で日本語化）
./target/release/deeprepo-slides-mcp slides \
  --flavor mdbook-reveal \
  --out ./out/slides \
  --sections "overview,architecture,modules" \
  --export "html" \
  --c deeprepo.toml

# 全機能を一度にビルド（推奨）
./target/release/deeprepo-slides-mcp build-all -c deeprepo.toml
```

## 主な実装内容

### 16並列処理（tech-book-readerの50並列翻訳を参考）

- **Semaphoreによる並列制御**: `tokio::sync::Semaphore::new(16)`で16並列に制限
- **モジュール単位の並列処理**: 各モジュールを並列処理し、結果を収集
- **日本語化処理**: 英語のコメントを日本語に翻訳（1センテンス形式）

### 1ページ1センテンス形式

- 各メソッドごとに1つのスライドを作成
- 説明を1センテンスにまとめる
- コードブロックと説明を組み合わせて表示

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

## ライセンス

MIT OR Apache-2.0
