# DeepRepoSlides MCP 仕様書

このディレクトリには、DeepRepoSlides MCPプロジェクトの実装が含まれています。

## プロジェクト構造

```
DeepRepoSlides/
├── Cargo.toml                 # ワークスペース設定
├── README.md                  # プロジェクト概要
├── deeprepo.toml.example      # 設定ファイルの例
├── crates/
│   ├── config/                # 設定ファイルパース
│   ├── mcp-server/            # MCPサーバー（JSON-RPC）
│   ├── analyzer-core/         # コード解析
│   ├── summarizer/            # 要約生成
│   ├── diagrammer/            # 図表生成
│   ├── site-mdbook/           # mdBookサイト生成
│   ├── slides/                # スライド生成
│   └── publisher-ghpages/     # GitHub Pages公開
└── apps/
    └── cli/                   # CLIアプリケーション
```

## ビルド方法

```bash
cargo build --release
```

## 使用方法

### MCPサーバーとして実行

```bash
export RUN_AS_MCP=1
./target/release/deeprepo-slides-mcp
```

### CLIとして実行

```bash
# リポジトリをインデックス化
./target/release/deeprepo-slides-mcp index --repo . -c deeprepo.toml

# Wikiを生成
./target/release/deeprepo-slides-mcp wiki --out ./out/wiki

# スライドを生成
./target/release/deeprepo-slides-mcp slides --flavor mdbook-reveal --out ./out/slides

# 全機能を一度にビルド
./target/release/deeprepo-slides-mcp build-all -c deeprepo.toml
```

## 設定ファイル

`deeprepo.toml.example`を参考に、プロジェクトルートに`deeprepo.toml`を作成してください。

## 開発

各クレートは独立して開発・テスト可能です。詳細は各クレートのソースコードを参照してください。

