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

        // スライドコンテンツを生成
        for section in sections {
            self.generate_reveal_section(index, &src_dir, section).await?;
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

        let mut files = Vec::new();
        let marp_content = self.generate_marp_content(index, sections).await?;
        let marp_file = out_dir.join("slides.md");
        fs::write(&marp_file, marp_content)?;

        // Marp CLIでビルド
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
                }
                _ => {}
            }

            let output = cmd.output().with_context(|| {
                "Marp CLIが見つかりません。インストールしてください: npm install -g @marp-team/marp-cli"
            })?;

            if output.status.success() && output_file.exists() {
                files.push(SlideFile {
                    format: format.clone(),
                    path: output_file,
                });
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

    /// reveal用のセクションを生成
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

    /// 概要スライドを生成
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

    /// Marpコンテンツを生成
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

