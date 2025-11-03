/**
 * スライド生成実装
 * 
 * コードベースからスライドを生成する
 * - mdbook-revealプラグインを使用
 * - Marp CLIを使用（オプション）
 * - HTML/PDF/PPTX形式でエクスポート
 * 
 * 主な仕様:
 * - mdbook-revealをデフォルトとして使用
 * - Marpは外部コマンド（Node.js依存）
 * - タイトル、全体構成、モジュール、シーケンス、運用、リスクのセクション
 * 
 * 制限事項:
 * - mdbook-revealはmdBookプロジェクトから生成
 * - Marpは別途インストールが必要
 */

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;
use anyhow::{Context, Result};
use tracing::{info, warn};

use config::Config;
use analyzer_core::Index;
use summarizer::Summarizer;
use diagrammer::Diagrammer;

/// スライドビルダー
pub struct SlideBuilder {
    config: Config,
    #[allow(dead_code)]
    summarizer: Summarizer,
    diagrammer: Diagrammer,
}

impl SlideBuilder {
    /// 新しいスライドビルダーインスタンスを作成
    /// 
    /// # 引数
    /// * `config` - 設定
    /// 
    /// # 戻り値
    /// * `Self` - スライドビルダーインスタンス
    pub fn new(config: Config) -> Self {
        Self {
            config: config.clone(),
            summarizer: Summarizer::new(config.clone()),
            diagrammer: Diagrammer::new(config.clone()),
        }
    }

    /// スライドをビルド
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// * `flavor` - フレーバー（mdbook-reveal|marp）
    /// * `out_dir` - 出力ディレクトリ
    /// * `sections` - セクションのリスト
    /// * `export` - エクスポート形式のリスト（html|pdf|pptx）
    /// 
    /// # 戻り値
    /// * `Result<SlideResult>` - ビルド結果、またはエラー
    pub async fn build_slides(
        &self,
        index: &Index,
        flavor: &str,
        out_dir: &str,
        sections: &[String],
        export: &[String],
    ) -> Result<SlideResult> {
        info!("スライドビルド開始: flavor={}, out_dir={}", flavor, out_dir);

        let out_path = PathBuf::from(out_dir);
        fs::create_dir_all(&out_path)?;

        match flavor {
            "mdbook-reveal" => self.build_mdbook_reveal(index, &out_path, sections, export).await,
            "marp" => self.build_marp(index, &out_path, sections, export).await,
            _ => Err(anyhow::anyhow!("不明なフレーバー: {}", flavor)),
        }
    }

    /// mdbook-revealでスライドをビルド
    async fn build_mdbook_reveal(
        &self,
        index: &Index,
        out_dir: &Path,
        sections: &[String],
        _export: &[String],
    ) -> Result<SlideResult> {
        info!("mdbook-revealでスライドをビルド中...");

        let src_dir = out_dir.join("src");
        fs::create_dir_all(&src_dir)?;

        // book.tomlを生成（revealプラグイン設定付き）
        self.generate_reveal_book_toml(out_dir)?;

        // SUMMARY.mdを生成
        self.generate_reveal_summary(&src_dir, sections)?;

        // スライドコンテンツを並列生成（16並列対応）
        // インデックスと設定をクローンして各セクションで使用可能にする
        let index_clone = index.clone();
        let config_clone = self.config.clone();
        
        // 各セクションの生成を並列実行
        let mut section_handles = Vec::new();
        for section in sections {
            let section = section.clone();
            let src_dir_clone = src_dir.clone();
            let index_for_section = index_clone.clone();
            let config_for_section = config_clone.clone();
            
            let handle = tokio::spawn(async move {
                // 各セクション用に新しいインスタンスを作成
                let summarizer = Summarizer::new(config_for_section.clone());
                let diagrammer = Diagrammer::new(config_for_section.clone());
                
                Self::generate_reveal_section_parallel(
                    &index_for_section,
                    &src_dir_clone,
                    &section,
                    &summarizer,
                    &diagrammer,
                ).await
            });
            section_handles.push(handle);
        }
        
        // すべてのセクションを並列実行して結果を収集
        for handle in section_handles {
            handle.await??;
        }

        // mdbook buildを実行
        let output = Command::new("mdbook")
            .arg("build")
            .current_dir(out_dir)
            .output()
            .with_context(|| {
                "mdBookコマンドが見つかりません。インストールしてください: cargo install mdbook"
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("mdBookビルドエラー: {}", stderr));
        }

        let mut files = Vec::new();
        let html_path = out_dir.join("book").join("index.html");
        if html_path.exists() {
            files.push(SlideFile {
                format: "html".to_string(),
                path: html_path,
            });
        }

        Ok(SlideResult {
            ok: true,
            files,
        })
    }

    /// Marpでスライドをビルド
    async fn build_marp(
        &self,
        index: &Index,
        out_dir: &Path,
        sections: &[String],
        export: &[String],
    ) -> Result<SlideResult> {
        info!("Marpでスライドをビルド中...");

        // Marpコンテンツを並列生成（16並列対応）
        let index_clone = index.clone();
        let config_clone = self.config.clone();
        
        // 各セクションの生成を並列実行
        let mut section_handles = Vec::new();
        for section in sections {
            let section = section.clone();
            let index_for_section = index_clone.clone();
            let config_for_section = config_clone.clone();
            
            let handle = tokio::spawn(async move {
                let summarizer = Summarizer::new(config_for_section.clone());
                let diagrammer = Diagrammer::new(config_for_section.clone());
                
                match section.as_str() {
                    "overview" => Self::generate_overview_slide_parallel(&index_for_section, &summarizer, &diagrammer).await,
                    "architecture" => Self::generate_architecture_slide_parallel(&index_for_section, &summarizer, &diagrammer).await,
                    "modules" => Self::generate_modules_slide_parallel(&index_for_section, &summarizer).await,
                    "flows" => Self::generate_flows_slide_parallel(&index_for_section, &diagrammer).await,
                    "deploy" => Self::generate_deploy_slide_parallel(&index_for_section, &diagrammer).await,
                    _ => Ok(format!("# {}\n\nセクションの内容\n", section)),
                }
            });
            section_handles.push(handle);
        }
        
        // すべてのセクションを並列実行して結果を収集
        let mut marp_content = String::from("---\nmarp: true\ntheme: default\n---\n\n");
        for handle in section_handles {
            let section_content = handle.await??;
            marp_content.push_str(&section_content);
            marp_content.push_str("\n");
        }
        
        let marp_file = out_dir.join("slides.md");
        fs::write(&marp_file, marp_content)?;

        // Marp CLIでビルド
        let mut files = Vec::new();
        for format in export {
            let output_file = match format.as_str() {
                "html" => out_dir.join("slides.html"),
                "pdf" => out_dir.join("slides.pdf"),
                "pptx" => out_dir.join("slides.pptx"),
                _ => {
                    warn!("サポートされていない形式: {}", format);
                    continue;
                }
            };

            let mut cmd = Command::new("marp");
            cmd.arg(&marp_file)
                .arg("--output")
                .arg(&output_file);

            match format.as_str() {
                "html" => {
                    cmd.arg("--html");
                }
                "pdf" => {
                    cmd.arg("--pdf");
                }
                "pptx" => {
                    cmd.arg("--pptx");
                    // .pptx形式の生成を確実にするため、エラーハンドリングを改善
                    cmd.arg("--allow-local-files");
                }
                _ => {}
            }

            let output = cmd.output().with_context(|| {
                format!("Marp CLIが見つかりません。インストールしてください: npm install -g @marp-team/marp-cli")
            })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("Marp CLIエラー (形式: {}): {}", format, stderr);
                // .pptx形式の場合は、エラーがあっても続行
                if format != "pptx" {
                    return Err(anyhow::anyhow!("Marp CLIビルドエラー (形式: {}): {}", format, stderr));
                }
            }

            if output_file.exists() {
                files.push(SlideFile {
                    format: format.clone(),
                    path: output_file,
                });
            } else if format == "pptx" {
                // .pptx形式の生成に失敗した場合の警告
                warn!("スライドファイルが生成されませんでした: {:?}", output_file);
            }
        }

        Ok(SlideResult {
            ok: true,
            files,
        })
    }

    /// reveal用のbook.tomlを生成
    fn generate_reveal_book_toml(&self, out_dir: &Path) -> Result<()> {
        let book_toml = format!(
            r#"[book]
title = "{}"
authors = ["DeepRepoSlides"]
language = "ja"

[build]
build-dir = "book"

[output.html]
default-theme = "black"

[output.reveal]
"#,
            self.config.project.name
        );

        let book_toml_path = out_dir.join("book.toml");
        fs::write(&book_toml_path, book_toml)
            .with_context(|| format!("book.tomlの書き込みに失敗しました: {:?}", book_toml_path))?;

        Ok(())
    }

    /// reveal用のSUMMARY.mdを生成
    fn generate_reveal_summary(&self, src_dir: &Path, sections: &[String]) -> Result<()> {
        let mut summary = String::from("# Summary\n\n");

        for section in sections {
            let section_name = self.get_section_name(section);
            let file_name = format!("{}.md", section);
            summary.push_str(&format!("- [{}]({})\n", section_name, file_name));
        }

        let summary_path = src_dir.join("SUMMARY.md");
        fs::write(&summary_path, summary)
            .with_context(|| format!("SUMMARY.mdの書き込みに失敗しました: {:?}", summary_path))?;

        Ok(())
    }

    /// reveal用のセクションを並列実行用に生成（静的メソッド）
    async fn generate_reveal_section_parallel(
        index: &Index,
        src_dir: &Path,
        section: &str,
        summarizer: &Summarizer,
        diagrammer: &Diagrammer,
    ) -> Result<()> {
        let content = match section {
            "overview" => Self::generate_overview_slide_parallel(index, summarizer, diagrammer).await?,
            "architecture" => Self::generate_architecture_slide_parallel(index, summarizer, diagrammer).await?,
            "modules" => Self::generate_modules_slide_parallel(index, summarizer).await?,
            "flows" => Self::generate_flows_slide_parallel(index, diagrammer).await?,
            "deploy" => Self::generate_deploy_slide_parallel(index, diagrammer).await?,
            _ => format!("# {}\n\nセクションの内容\n", section),
        };

        let file_path = src_dir.join(format!("{}.md", section));
        fs::write(&file_path, content)
            .with_context(|| format!("セクションファイルの書き込みに失敗しました: {:?}", file_path))?;

        Ok(())
    }

    /// reveal用のセクションを生成（非並列実行用、後方互換性のため保持）
    #[allow(dead_code)] // 後方互換性のため保持
    async fn generate_reveal_section(
        &self,
        index: &Index,
        src_dir: &Path,
        section: &str,
    ) -> Result<()> {
        let content = match section {
            "overview" => self.generate_overview_slide(index).await?,
            "architecture" => self.generate_architecture_slide(index).await?,
            "modules" => self.generate_modules_slide(index).await?,
            "flows" => self.generate_flows_slide(index).await?,
            "deploy" => self.generate_deploy_slide(index).await?,
            _ => format!("# {}\n\nセクションの内容\n", section),
        };

        let file_path = src_dir.join(format!("{}.md", section));
        fs::write(&file_path, content)
            .with_context(|| format!("セクションファイルの書き込みに失敗しました: {:?}", file_path))?;

        Ok(())
    }

    /// 概要スライドを並列実行用に生成（静的メソッド）
    async fn generate_overview_slide_parallel(
        index: &Index,
        summarizer: &Summarizer,
        diagrammer: &Diagrammer,
    ) -> Result<String> {
        let mut content = String::new();
        
        // タイトルスライド
        content.push_str("---\n");
        content.push_str(&format!("# {}\n\n", index.repo_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("プロジェクト")));
        
        // リポジトリ要約を取得
        let summary_result = summarizer.summarize(index, "repo", "", "concise-ja").await?;
        let summary_lines: Vec<&str> = summary_result.content_md.lines().take(5).collect();
        for line in summary_lines {
            if !line.trim().is_empty() {
                content.push_str(line);
                content.push_str("\n");
            }
        }
        content.push_str("\n");
        
        content.push_str(&format!(
            "📊 **統計**: {}ファイル、{}言語、{}モジュール\n",
            index.stats.files,
            index.stats.languages.len(),
            index.stats.modules
        ));
        content.push_str("---\n\n");
        
        // 全体構成図
        content.push_str("---\n");
        content.push_str("## 全体構成\n\n");
        let diagram = diagrammer.generate_diagram(index, "module-graph")?;
        if diagram.format == "mermaid" {
            content.push_str(&format!("```mermaid\n{}\n```\n", diagram.content));
        }
        content.push_str("---\n\n");

        Ok(content)
    }

    /// アーキテクチャスライドを並列実行用に生成（静的メソッド）
    async fn generate_architecture_slide_parallel(
        index: &Index,
        summarizer: &Summarizer,
        diagrammer: &Diagrammer,
    ) -> Result<String> {
        let mut content = String::new();

        content.push_str("---\n");
        content.push_str("## アーキテクチャ概要\n");
        content.push_str("---\n\n");
        
        // アーキテクチャ要約を取得
        let summary_result = summarizer.summarize(index, "repo", "", "concise-ja").await?;
        let summary_lines: Vec<&str> = summary_result.content_md.lines().take(10).collect();
        for line in summary_lines {
            if !line.trim().is_empty() {
                content.push_str(line);
                content.push_str("\n");
            }
        }
        content.push_str("\n---\n\n");

        // モジュールグラフ図
        content.push_str("---\n");
        content.push_str("### モジュール構成図\n\n");
        let diagram = diagrammer.generate_diagram(index, "module-graph")?;
        if diagram.format == "mermaid" {
            content.push_str(&format!("```mermaid\n{}\n```\n", diagram.content));
        }
        content.push_str("---\n\n");

        // 主要モジュール一覧
        content.push_str("---\n");
        content.push_str("### 主要モジュール\n\n");
        for (i, module) in index.modules.iter().take(10).enumerate() {
            content.push_str(&format!("{}. **{}**\n", i + 1, module.name));
            content.push_str(&format!("   - パス: `{}`\n", module.path.display()));
            content.push_str(&format!("   - 言語: {}\n", module.language));
            if !module.dependencies.is_empty() {
                content.push_str(&format!("   - 依存: {}\n", module.dependencies.join(", ")));
            }
            content.push_str("\n");
        }
        content.push_str("---\n\n");

        Ok(content)
    }

    /// モジュールスライドを並列実行用に生成（静的メソッド）
    async fn generate_modules_slide_parallel(
        index: &Index,
        summarizer: &Summarizer,
    ) -> Result<String> {
        let mut content = String::new();

        content.push_str("---\n");
        content.push_str("## モジュール詳細\n");
        content.push_str("---\n\n");

        // モジュールごとにスライドを生成
        for (idx, module) in index.modules.iter().take(20).enumerate() {
            if idx > 0 {
                content.push_str("---\n\n");
            }
            
            content.push_str(&format!("### {}\n\n", module.name));
            content.push_str(&format!("**パス**: `{}`\n\n", module.path.display()));
            content.push_str(&format!("**言語**: {}\n\n", module.language));
            
            if !module.dependencies.is_empty() {
                content.push_str("**依存関係**:\n");
                for dep in &module.dependencies {
                    content.push_str(&format!("- `{}`\n", dep));
                }
                content.push_str("\n");
            }
            
            // モジュールの要約を生成
            let summary_result = summarizer
                .summarize(index, "module", &module.path.to_string_lossy(), "concise-ja")
                .await?;
            let summary_lines: Vec<&str> = summary_result.content_md.lines().take(10).collect();
            for line in summary_lines {
                if !line.trim().is_empty() {
                    content.push_str(line);
                    content.push_str("\n");
                }
            }
            content.push_str("\n");
        }

        Ok(content)
    }

    /// フロースライドを並列実行用に生成（静的メソッド）
    async fn generate_flows_slide_parallel(
        index: &Index,
        diagrammer: &Diagrammer,
    ) -> Result<String> {
        let mut content = String::new();

        content.push_str("---\n");
        content.push_str("## システムフロー\n");
        content.push_str("---\n\n");

        // シーケンス図
        content.push_str("---\n");
        content.push_str("### シーケンス図\n\n");
        let diagram = diagrammer.generate_diagram(index, "sequence")?;
        if diagram.format == "mermaid" {
            content.push_str(&format!("```mermaid\n{}\n```\n", diagram.content));
        }
        content.push_str("---\n\n");

        // コールグラフ
        content.push_str("---\n");
        content.push_str("### コールグラフ\n\n");
        let diagram = diagrammer.generate_diagram(index, "call-graph")?;
        if diagram.format == "mermaid" {
            content.push_str(&format!("```mermaid\n{}\n```\n", diagram.content));
        }
        content.push_str("---\n\n");

        Ok(content)
    }

    /// デプロイスライドを並列実行用に生成（静的メソッド）
    async fn generate_deploy_slide_parallel(
        index: &Index,
        diagrammer: &Diagrammer,
    ) -> Result<String> {
        let mut content = String::new();

        content.push_str("---\n");
        content.push_str("## デプロイメント構成\n");
        content.push_str("---\n\n");

        // デプロイメント図
        let diagram = diagrammer.generate_diagram(index, "deployment")?;
        if diagram.format == "mermaid" {
            content.push_str(&format!("```mermaid\n{}\n```\n", diagram.content));
        }
        content.push_str("\n---\n\n");

        // エントリーポイント
        content.push_str("---\n");
        content.push_str("### エントリーポイント\n\n");
        if !index.entrypoints.is_empty() {
            for ep in &index.entrypoints {
                content.push_str(&format!("- `{}`\n", ep.display()));
            }
        } else {
            content.push_str("エントリーポイントが見つかりませんでした。\n");
        }
        content.push_str("\n---\n\n");

        Ok(content)
    }

    /// 概要スライドを生成（非並列実行用、後方互換性のため保持）
    async fn generate_overview_slide(&self, index: &Index) -> Result<String> {
        let mut content = String::new();

        content.push_str("---\n");
        content.push_str(&format!("# {}\n\n", self.config.project.name));
        content.push_str(&format!(
            "{}ファイル、{}言語、{}モジュール\n",
            index.stats.files,
            index.stats.languages.len(),
            index.stats.modules
        ));
        content.push_str("---\n\n");

        content.push_str("## 全体構成\n\n");
        let diagram = self.diagrammer.generate_diagram(index, "module-graph")?;
        if diagram.format == "mermaid" {
            content.push_str(&format!("```mermaid\n{}\n```\n", diagram.content));
        }

        Ok(content)
    }

    /// アーキテクチャスライドを生成
    async fn generate_architecture_slide(&self, index: &Index) -> Result<String> {
        let mut content = String::new();

        content.push_str("---\n");
        content.push_str("## アーキテクチャ\n");
        content.push_str("---\n\n");

        content.push_str("### 主要モジュール\n\n");
        for module in &index.modules {
            content.push_str(&format!("- **{}**\n", module.name));
        }

        Ok(content)
    }

    /// モジュールスライドを生成
    async fn generate_modules_slide(&self, index: &Index) -> Result<String> {
        let mut content = String::new();

        content.push_str("---\n");
        content.push_str("## モジュール\n");
        content.push_str("---\n\n");

        for module in &index.modules {
            content.push_str(&format!("### {}\n\n", module.name));
            content.push_str(&format!("パス: `{}`\n\n", module.path.display()));
            if !module.dependencies.is_empty() {
                content.push_str("依存関係:\n");
                for dep in &module.dependencies {
                    content.push_str(&format!("- `{}`\n", dep));
                }
            }
            content.push_str("\n---\n\n");
        }

        Ok(content)
    }

    /// フロースライドを生成
    async fn generate_flows_slide(&self, index: &Index) -> Result<String> {
        let mut content = String::new();

        content.push_str("---\n");
        content.push_str("## フロー\n");
        content.push_str("---\n\n");

        content.push_str("### シーケンス図\n\n");
        let diagram = self.diagrammer.generate_diagram(index, "sequence")?;
        if diagram.format == "mermaid" {
            content.push_str(&format!("```mermaid\n{}\n```\n", diagram.content));
        }

        Ok(content)
    }

    /// デプロイスライドを生成
    async fn generate_deploy_slide(&self, index: &Index) -> Result<String> {
        let mut content = String::new();

        content.push_str("---\n");
        content.push_str("## デプロイ\n");
        content.push_str("---\n\n");

        content.push_str("### エントリーポイント\n\n");
        for ep in &index.entrypoints {
            content.push_str(&format!("- `{}`\n", ep.display()));
        }

        Ok(content)
    }

    /// Marpコンテンツを生成（非並列実行用、後方互換性のため保持）
    #[allow(dead_code)] // 後方互換性のため保持
    async fn generate_marp_content(&self, index: &Index, sections: &[String]) -> Result<String> {
        let mut content = String::from("---\nmarp: true\ntheme: default\n---\n\n");

        for section in sections {
            match section.as_str() {
                "overview" => {
                    content.push_str(&self.generate_overview_slide(index).await?);
                }
                "architecture" => {
                    content.push_str(&self.generate_architecture_slide(index).await?);
                }
                "modules" => {
                    content.push_str(&self.generate_modules_slide(index).await?);
                }
                "flows" => {
                    content.push_str(&self.generate_flows_slide(index).await?);
                }
                "deploy" => {
                    content.push_str(&self.generate_deploy_slide(index).await?);
                }
                _ => {}
            }
        }

        Ok(content)
    }

    /// セクション名を取得
    fn get_section_name<'a>(&self, section: &'a str) -> &'a str {
        match section {
            "overview" => "概要",
            "architecture" => "アーキテクチャ",
            "modules" => "モジュール",
            "flows" => "フロー",
            "deploy" => "デプロイ",
            _ => section,
        }
    }
}

/// スライドビルド結果
#[derive(Debug, Serialize, Deserialize)]
pub struct SlideResult {
    pub ok: bool,
    pub files: Vec<SlideFile>,
}

/// スライドファイル
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideFile {
    pub format: String,
    pub path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_section_name() {
        let config = Config::default();
        let builder = SlideBuilder::new(config);
        assert_eq!(builder.get_section_name("overview"), "概要");
    }
}

