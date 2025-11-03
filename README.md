# DeepRepoSlides MCP

Rust製のMCPツールで、ローカル/Mono-Repoを静的解析して日本語要約を作成し、DeepWiki風のドキュメントサイトとスライドを自動生成してGitHub Pagesで公開できるようにします。

## 機能

- **多言語対応の静的解析**: TypeScript/JavaScript, Python, Go, Rust, Javaなど
- **日本語要約生成**: LLMまたは静的ヒューリスティックによる要約
- **DeepWiki風ドキュメント生成**: mdBookベースのWikiサイト（Mermaid対応）
- **スライド生成**: mdbook-revealまたはMarpによるスライド生成
- **GitHub Pages連携**: docs/またはgh-pagesブランチへの自動公開

## セットアップ

```bash
cargo build --release
```

ビルド後、以下のいずれかの方法で実行できます：

### 方法1: ビルド済みバイナリを直接実行（推奨）

```bash
# MCPサーバーとして実行
export RUN_AS_MCP=1
./target/release/deeprepo-slides-mcp

# CLIとして実行
./target/release/deeprepo-slides-mcp index --repo ../my-repo -c deeprepo.toml
./target/release/deeprepo-slides-mcp wiki --out ./out/wiki
./target/release/deeprepo-slides-mcp slides --flavor mdbook-reveal --out ./out/slides
./target/release/deeprepo-slides-mcp publish --mode docs
```

### 方法2: cargo runで実行（開発時）

```bash
# MCPサーバーとして実行
export RUN_AS_MCP=1
cargo run --release

# CLIとして実行
cargo run --release -- index --repo ../my-repo -c deeprepo.toml
cargo run --release -- wiki --out ./out/wiki
cargo run --release -- slides --flavor mdbook-reveal --out ./out/slides
cargo run --release -- publish --mode docs
```

### 方法3: システムにインストール（PATHに追加）

```bash
# インストール（デフォルトでは ~/.cargo/bin にインストールされます）
cargo install --path .

# インストール後はどこからでも実行可能
export RUN_AS_MCP=1
deeprepo-slides-mcp
```

## 設定ファイル

`deeprepo.toml`をプロジェクトルートに配置してください。詳細は仕様書を参照してください。

## ライセンス

MIT OR Apache-2.0

