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

## 使用方法

### MCPサーバーとして実行

```bash
export RUN_AS_MCP=1
deeprepo-slides-mcp
```

### CLIとして実行

```bash
deeprepo-slides-mcp index --repo ../my-repo -c deeprepo.toml
deeprepo-slides-mcp wiki --out ./out/wiki
deeprepo-slides-mcp slides --flavor mdbook-reveal --out ./out/slides
deeprepo-slides-mcp publish --mode docs
```

## 設定ファイル

`deeprepo.toml`をプロジェクトルートに配置してください。詳細は仕様書を参照してください。

## ライセンス

MIT OR Apache-2.0

