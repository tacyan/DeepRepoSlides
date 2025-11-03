/**
 * CLIアプリケーション
 * 
 * MCPサーバーとして動作するか、直接CLIコマンドとして実行できる
 * - RUN_AS_MCP環境変数が設定されている場合はMCPサーバーとして動作
 * - それ以外の場合はCLIコマンドとして動作
 * 
 * 主な仕様:
 * - index: リポジトリをインデックス化
 * - summarize: 要約を生成
 * - wiki: Wikiサイトを生成
 * - slides: スライドを生成
 * - publish: GitHub Pagesに公開
 * 
 * 制限事項:
 * - MCPモードでは標準入出力でJSON-RPC通信
 * - CLIモードではコマンドライン引数で操作
 */

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

use config::Config;
use mcp_server::McpServer;
use analyzer_core::Analyzer;
use site_mdbook::MdBookBuilder;
use slides::SlideBuilder;
use publisher_ghpages::Publisher;

#[tokio::main]
async fn main() -> Result<()> {
    // ログ設定
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(EnvFilter::from_default_env())
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // MCPサーバーモード
    if std::env::var("RUN_AS_MCP").is_ok() {
        return run_mcp_server().await;
    }

    // CLIモード
    run_cli().await
}

/// MCPサーバーを起動
async fn run_mcp_server() -> Result<()> {
    info!("MCPサーバーモードで起動");

    let config = Config::load::<PathBuf>(None)?;
    let server = McpServer::new(config);
    server.serve().await?;

    Ok(())
}

/// CLIコマンドを実行
async fn run_cli() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Index { repo, config } => {
            cmd_index(&repo, config.as_deref()).await?;
        }
        Commands::Summarize { scope, target, style } => {
            cmd_summarize(&scope, &target, &style).await?;
        }
        Commands::Wiki { out, config } => {
            cmd_wiki(out.as_deref(), config.as_deref()).await?;
        }
        Commands::Slides {
            flavor,
            out,
            sections,
            export,
            config,
        } => {
            let sections_vec: Vec<String> = sections.split(',').map(|s| s.trim().to_string()).collect();
            let export_vec: Vec<String> = export.split(',').map(|s| s.trim().to_string()).collect();
            cmd_slides(
                &flavor,
                out.as_deref(),
                &sections_vec,
                &export_vec,
                config.as_deref(),
            )
            .await?;
        }
        Commands::Publish {
            mode,
            site_dir,
            slides_dir,
            repo_root,
            branch,
        } => {
            cmd_publish(
                &mode,
                &site_dir,
                &slides_dir,
                &repo_root,
                &branch,
            )
            .await?;
        }
        Commands::BuildAll { config } => {
            cmd_build_all(config.as_deref()).await?;
        }
    }

    Ok(())
}

/// indexコマンドを実行
async fn cmd_index(repo: &str, config_path: Option<&str>) -> Result<()> {
    info!("リポジトリをインデックス化: {}", repo);

    let config = Config::load(config_path)?;
    let analyzer = Analyzer::new(config.clone());
    let index = analyzer.analyze_repo(repo, &config).await?;

    println!("インデックス化完了:");
    println!("  ファイル数: {}", index.stats.files);
    println!("  言語数: {}", index.stats.languages.len());
    println!("  モジュール数: {}", index.stats.modules);

    Ok(())
}

/// summarizeコマンドを実行
async fn cmd_summarize(scope: &str, target: &str, style: &str) -> Result<()> {
    info!("要約生成: scope={}, target={}, style={}", scope, target, style);

    // インデックスを読み込む（簡易実装）
    // 実際の実装では、インデックスを保存・読み込む機能が必要
    eprintln!("要約機能は実装中です");

    Ok(())
}

/// wikiコマンドを実行
async fn cmd_wiki(out: Option<&str>, config_path: Option<&str>) -> Result<()> {
    let out_dir = out.unwrap_or("./out/wiki");
    info!("Wiki生成: out_dir={}", out_dir);

    let _config = Config::load(config_path)?;
    
    // インデックスを読み込む（簡易実装）
    // 実際の実装では、インデックスを保存・読み込む機能が必要
    eprintln!("Wiki生成機能は実装中です（インデックスが必要です）");

    Ok(())
}

/// slidesコマンドを実行
async fn cmd_slides(
    flavor: &str,
    out: Option<&str>,
    _sections: &[String],
    _export: &[String],
    config_path: Option<&str>,
) -> Result<()> {
    let out_dir = out.unwrap_or("./out/slides");
    info!("スライド生成: flavor={}, out_dir={}", flavor, out_dir);

    let _config = Config::load(config_path)?;
    
    // インデックスを読み込む（簡易実装）
    // 実際の実装では、インデックスを保存・読み込む機能が必要
    eprintln!("スライド生成機能は実装中です（インデックスが必要です）");

    Ok(())
}

/// publishコマンドを実行
async fn cmd_publish(
    mode: &str,
    site_dir: &str,
    slides_dir: &str,
    repo_root: &str,
    branch: &str,
) -> Result<()> {
    info!("GitHub Pages公開: mode={}", mode);

    let config = Config::load::<PathBuf>(None)?;
    let publisher = Publisher::new(config);
    let result = publisher
        .publish(mode, site_dir, slides_dir, repo_root, branch)
        .await?;

    println!("公開完了: {}", result.hint);

    Ok(())
}

/// build-allコマンドを実行（全機能を一度に実行）
async fn cmd_build_all(config_path: Option<&str>) -> Result<()> {
    info!("全機能をビルド中...");

    let config = Config::load(config_path)?;
    
    // 1. インデックス化
    info!("1. リポジトリをインデックス化中...");
    let analyzer = Analyzer::new(config.clone());
    let index = analyzer.analyze_repo(&config.project.repo_path, &config).await?;
    
    println!("インデックス化完了: {}ファイル, {}モジュール", index.stats.files, index.stats.modules);

    // 2. Wiki生成
    info!("2. Wikiを生成中...");
    let wiki_builder = MdBookBuilder::new(config.clone());
    let wiki_result = wiki_builder
        .build_wiki(
            &index,
            &config.site.out_dir.to_string_lossy(),
            true,
            &vec!["overview", "architecture", "modules", "flows", "deploy", "faq"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
        )
        .await?;
    
    println!("Wiki生成完了: {}ページ", wiki_result.pages);

    // 3. スライド生成
    info!("3. スライドを生成中...");
    let slide_builder = SlideBuilder::new(config.clone());
    let slide_result = slide_builder
        .build_slides(
            &index,
            &config.slides.flavor,
            &config.slides.out_dir.to_string_lossy(),
            &vec!["overview", "architecture", "modules"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            &vec!["html"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
        )
        .await?;
    
    println!("スライド生成完了: {}ファイル", slide_result.files.len());

    // 4. GitHub Pages公開（オプション）
    if config.publish.mode == "docs" {
        info!("4. GitHub Pagesに公開中...");
        let publisher = Publisher::new(config.clone());
        let slides_out_dir = config.slides.out_dir.to_string_lossy().to_string();
        let publish_result = publisher
            .publish(
                "docs",
                &wiki_result.site_dir.to_string_lossy(),
                &slides_out_dir,
                ".",
                "gh-pages",
            )
            .await?;
        
        println!("公開完了: {}", publish_result.hint);
    }

    println!("全機能のビルドが完了しました！");

    Ok(())
}

/// CLI引数定義
#[derive(Parser)]
#[command(name = "deeprepo-slides-mcp")]
#[command(about = "DeepRepoSlides MCP - リポジトリ解析とWiki/スライド生成ツール")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// コマンド定義
#[derive(Subcommand)]
enum Commands {
    /// リポジトリをインデックス化
    Index {
        /// リポジトリパス
        #[arg(long)]
        repo: String,

        /// 設定ファイルパス
        #[arg(short, long)]
        config: Option<String>,
    },

    /// 要約を生成
    Summarize {
        /// スコープ（repo|package|module|file）
        #[arg(long)]
        scope: String,

        /// 対象（パスまたはモジュールID）
        #[arg(long)]
        target: String,

        /// スタイル（concise-ja|detailed-ja）
        #[arg(long, default_value = "concise-ja")]
        style: String,
    },

    /// Wikiサイトを生成
    Wiki {
        /// 出力ディレクトリ
        #[arg(short, long)]
        out: Option<String>,

        /// 設定ファイルパス
        #[arg(short, long)]
        config: Option<String>,
    },

    /// スライドを生成
    Slides {
        /// フレーバー（mdbook-reveal|marp）
        #[arg(long, default_value = "mdbook-reveal")]
        flavor: String,

        /// 出力ディレクトリ
        #[arg(short, long)]
        out: Option<String>,

        /// セクション
        #[arg(long, default_value = "overview,architecture,modules")]
        sections: String,

        /// エクスポート形式（html|pdf|pptx）
        #[arg(long, default_value = "html")]
        export: String,

        /// 設定ファイルパス
        #[arg(short, long)]
        config: Option<String>,
    },

    /// GitHub Pagesに公開
    Publish {
        /// モード（docs|gh-pages）
        #[arg(long, default_value = "docs")]
        mode: String,

        /// サイトディレクトリ
        #[arg(long)]
        site_dir: String,

        /// スライドディレクトリ
        #[arg(long)]
        slides_dir: String,

        /// リポジトリルート
        #[arg(long, default_value = ".")]
        repo_root: String,

        /// ブランチ名（gh-pagesモードの場合）
        #[arg(long, default_value = "gh-pages")]
        branch: String,
    },

    /// 全機能を一度にビルド（index + wiki + slides + publish）
    BuildAll {
        /// 設定ファイルパス
        #[arg(short, long)]
        config: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cli_parse() {
        // CLI引数のパーステスト
        let cli = Cli::parse_from(&["deeprepo-slides-mcp", "index", "--repo", "."]);
        match cli.command {
            Commands::Index { repo, .. } => {
                assert_eq!(repo, ".");
            }
            _ => panic!("予期しないコマンド"),
        }
    }
}

