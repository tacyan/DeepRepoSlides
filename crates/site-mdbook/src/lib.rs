/**
 * mdBookサイト生成実装
 * 
 * DeepWiki風のドキュメントサイトをmdBookで構築する
 * - book.tomlの自動生成
 * - SUMMARY.mdの生成
 * - 各章のMarkdownファイル生成
 * - Mermaid図の埋め込み
 * 
 * 主な仕様:
 * - Overview, Architecture, Modules, Flows, Deploy, FAQの章構成
 * - Mermaid対応のテーマ設定
 * - GitHub Pages対応（/docsディレクトリに出力可能）
 * 
 * 制限事項:
 * - mdBookは外部コマンドとして実行（crate APIは使用しない）
 * - カスタムテーマは最小限の設定のみ
 */

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;
use anyhow::{Context, Result};
use tracing::info;

use config::Config;
use analyzer_core::Index;
use summarizer::Summarizer;
use diagrammer::Diagrammer;

/// mdBookビルダー
pub struct MdBookBuilder {
    config: Config,
    #[allow(dead_code)] // 後方互換性のため保持（非並列実行時のgenerate_sectionメソッドで使用）
    summarizer: Summarizer,
    #[allow(dead_code)] // 後方互換性のため保持（非並列実行時のgenerate_sectionメソッドで使用）
    diagrammer: Diagrammer,
}

impl MdBookBuilder {
    /// 新しいmdBookビルダーインスタンスを作成
    /// 
    /// # 引数
    /// * `config` - 設定
    /// 
    /// # 戻り値
    /// * `Self` - mdBookビルダーインスタンス
    pub fn new(config: Config) -> Self {
        Self {
            config: config.clone(),
            summarizer: Summarizer::new(config.clone()),
            diagrammer: Diagrammer::new(config.clone()),
        }
    }

    /// Wikiをビルド
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// * `out_dir` - 出力ディレクトリ
    /// * `with_diagrams` - 図を含めるか
    /// * `toc` - 目次セクションのリスト
    /// 
    /// # 戻り値
    /// * `Result<WikiResult>` - ビルド結果、またはエラー
    pub async fn build_wiki(
        &self,
        index: &Index,
        out_dir: &str,
        with_diagrams: bool,
        toc: &[String],
    ) -> Result<WikiResult> {
        info!("Wikiビルド開始: out_dir={}", out_dir);

        let out_path = PathBuf::from(out_dir);
        let src_dir = out_path.join("src");
        fs::create_dir_all(&src_dir)?;

        // book.tomlを生成
        self.generate_book_toml(&out_path)?;

        // SUMMARY.mdを生成
        self.generate_summary(&src_dir, toc)?;

        // 各章を並列生成（16並列対応）
        // インデックスと設定をクローンして各セクションで使用可能にする
        let index_clone = index.clone();
        let config_clone = self.config.clone();
        
        // 各セクションの生成を並列実行
        let mut section_handles = Vec::new();
        for section in toc {
            let section = section.clone();
            let src_dir_clone = src_dir.clone();
            let with_diagrams = with_diagrams;
            let index_for_section = index_clone.clone();
            let config_for_section = config_clone.clone();
            
            let handle = tokio::spawn(async move {
                // 各セクション用に新しいインスタンスを作成
                let summarizer = Summarizer::new(config_for_section.clone());
                let diagrammer = Diagrammer::new(config_for_section.clone());
                
                Self::generate_section_parallel(
                    &index_for_section,
                    &src_dir_clone,
                    &section,
                    with_diagrams,
                    &summarizer,
                    &diagrammer,
                ).await
            });
            section_handles.push(handle);
        }
        
        // すべてのセクションを並列実行して結果を収集
        let mut pages = 0;
        for handle in section_handles {
            let page_count = handle.await??;
            pages += page_count;
        }

        // mdBookをビルド
        self.build_mdbook(&out_path)?;

        Ok(WikiResult {
            ok: true,
            site_dir: out_path.join("book"),
            pages,
        })
    }

    /// book.tomlを生成
    /// 
    /// # 引数
    /// * `out_dir` - 出力ディレクトリ
    /// 
    /// # 戻り値
    /// * `Result<()>` - 成功、またはエラー
    fn generate_book_toml(&self, out_dir: &Path) -> Result<()> {
        let book_toml = format!(
            r#"[book]
title = "{}"
authors = ["DeepRepoSlides"]
language = "ja"

[build]
build-dir = "book"

[output.html]
default-theme = "navy"
preferred-dark-theme = "navy"

[output.reveal]
optional = true
"#,
            self.config.project.name
        );

        let book_toml_path = out_dir.join("book.toml");
        fs::write(&book_toml_path, book_toml)
            .with_context(|| format!("book.tomlの書き込みに失敗しました: {:?}", book_toml_path))?;

        info!("book.tomlを生成しました: {:?}", book_toml_path);
        Ok(())
    }

    /// SUMMARY.mdを生成
    /// 
    /// # 引数
    /// * `src_dir` - ソースディレクトリ
    /// * `toc` - 目次セクションのリスト
    /// 
    /// # 戻り値
    /// * `Result<()>` - 成功、またはエラー
    fn generate_summary(&self, src_dir: &Path, toc: &[String]) -> Result<()> {
        let mut summary = String::from("# Summary\n\n");

        for section in toc {
            let section_name = self.get_section_name(section);
            let file_name = format!("{}.md", section);
            summary.push_str(&format!("- [{}]({})\n", section_name, file_name));
        }

        let summary_path = src_dir.join("SUMMARY.md");
        fs::write(&summary_path, summary)
            .with_context(|| format!("SUMMARY.mdの書き込みに失敗しました: {:?}", summary_path))?;

        info!("SUMMARY.mdを生成しました: {:?}", summary_path);
        Ok(())
    }

    /// セクション名を取得
    fn get_section_name<'a>(&self, section: &'a str) -> &'a str {
        match section {
            "overview" => "概要",
            "architecture" => "アーキテクチャ",
            "modules" => "モジュール",
            "flows" => "フロー",
            "deploy" => "デプロイ",
            "faq" => "FAQ",
            _ => section,
        }
    }

    /// セクションを並列実行用に生成（静的メソッド）
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// * `src_dir` - ソースディレクトリ
    /// * `section` - セクション名
    /// * `with_diagrams` - 図を含めるか
    /// * `summarizer` - サマライザー
    /// * `diagrammer` - ダイアグラマー
    /// 
    /// # 戻り値
    /// * `Result<usize>` - 生成されたページ数、またはエラー
    async fn generate_section_parallel(
        index: &Index,
        src_dir: &Path,
        section: &str,
        with_diagrams: bool,
        summarizer: &Summarizer,
        diagrammer: &Diagrammer,
    ) -> Result<usize> {
        let content = match section {
            "overview" => Self::generate_overview_parallel(index, summarizer).await?,
            "architecture" => Self::generate_architecture_parallel(index, with_diagrams, diagrammer).await?,
            "modules" => Self::generate_modules_parallel(index, summarizer).await?,
            "flows" => Self::generate_flows_parallel(index, with_diagrams, diagrammer).await?,
            "deploy" => Self::generate_deploy_parallel(index, diagrammer).await?,
            "faq" => Self::generate_faq_parallel(index).await?,
            _ => format!("# {}\n\nセクションの内容\n", section),
        };

        let page_count = match section {
            "modules" => index.modules.len().max(1),
            _ => 1,
        };

        let file_path = src_dir.join(format!("{}.md", section));
        fs::write(&file_path, content)
            .with_context(|| format!("セクションファイルの書き込みに失敗しました: {:?}", file_path))?;

        Ok(page_count)
    }

    /// セクションを生成
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// * `src_dir` - ソースディレクトリ
    /// * `section` - セクション名
    /// * `with_diagrams` - 図を含めるか
    /// 
    /// # 戻り値
    /// * `Result<usize>` - 生成されたページ数、またはエラー
    async fn generate_section(
        &self,
        index: &Index,
        src_dir: &Path,
        section: &str,
        with_diagrams: bool,
    ) -> Result<usize> {
        let content = match section {
            "overview" => self.generate_overview(index).await?,
            "architecture" => self.generate_architecture(index, with_diagrams).await?,
            "modules" => self.generate_modules(index).await?,
            "flows" => self.generate_flows(index, with_diagrams).await?,
            "deploy" => self.generate_deploy(index).await?,
            "faq" => self.generate_faq(index).await?,
            _ => format!("# {}\n\nセクションの内容\n", section),
        };

        let page_count = match section {
            "modules" => index.modules.len().max(1),
            _ => 1,
        };

        let file_path = src_dir.join(format!("{}.md", section));
        fs::write(&file_path, content)
            .with_context(|| format!("セクションファイルの書き込みに失敗しました: {:?}", file_path))?;

        Ok(page_count)
    }

    /// 概要セクションを並列実行用に生成
    async fn generate_overview_parallel(index: &Index, summarizer: &Summarizer) -> Result<String> {
        let summary_result = summarizer
            .summarize(index, "repo", "", "concise-ja")
            .await?;

        Ok(summary_result.content_md)
    }

    /// アーキテクチャセクションを並列実行用に生成
    async fn generate_architecture_parallel(
        index: &Index,
        with_diagrams: bool,
        diagrammer: &Diagrammer,
    ) -> Result<String> {
        let mut content = String::from("# アーキテクチャ\n\n");

        content.push_str("## システム構成\n\n");
        content.push_str(&format!(
            "このリポジトリは{}ファイル、{}言語、{}モジュールで構成されています。\n\n",
            index.stats.files,
            index.stats.languages.len(),
            index.stats.modules
        ));

        if with_diagrams {
            content.push_str("## モジュールグラフ\n\n");
            let diagram = diagrammer.generate_diagram(index, "module-graph")?;
            if diagram.format == "mermaid" {
                content.push_str(&format!("```mermaid\n{}\n```\n\n", diagram.content));
            }
        }

        content.push_str("## 主要コンポーネント\n\n");
        for module in &index.modules {
            content.push_str(&format!("- **{}** (`{}`)\n", module.name, module.path.display()));
        }

        Ok(content)
    }

    /// モジュールセクションを並列実行用に生成
    async fn generate_modules_parallel(index: &Index, summarizer: &Summarizer) -> Result<String> {
        let mut content = String::from("# モジュール\n\n");

        for module in &index.modules {
            content.push_str(&format!("## {}\n\n", module.name));
            content.push_str(&format!("パス: `{}`\n\n", module.path.display()));
            content.push_str(&format!("言語: {}\n\n", module.language));

            if !module.dependencies.is_empty() {
                content.push_str("### 依存関係\n\n");
                for dep in &module.dependencies {
                    content.push_str(&format!("- `{}`\n", dep));
                }
                content.push_str("\n");
            }

            // モジュールの要約を生成
            let summary_result = summarizer
                .summarize(
                    index,
                    "module",
                    &module.path.to_string_lossy(),
                    "concise-ja",
                )
                .await?;
            content.push_str(&summary_result.content_md);
            content.push_str("\n\n");
        }

        Ok(content)
    }

    /// フローセクションを並列実行用に生成
    async fn generate_flows_parallel(
        index: &Index,
        with_diagrams: bool,
        diagrammer: &Diagrammer,
    ) -> Result<String> {
        let mut content = String::from("# フロー\n\n");

        if with_diagrams {
            content.push_str("## シーケンス図\n\n");
            let diagram = diagrammer.generate_diagram(index, "sequence")?;
            if diagram.format == "mermaid" {
                content.push_str(&format!("```mermaid\n{}\n```\n\n", diagram.content));
            }

            content.push_str("## コールグラフ\n\n");
            let diagram = diagrammer.generate_diagram(index, "call-graph")?;
            if diagram.format == "mermaid" {
                content.push_str(&format!("```mermaid\n{}\n```\n\n", diagram.content));
            }
        }

        Ok(content)
    }

    /// デプロイセクションを並列実行用に生成
    async fn generate_deploy_parallel(index: &Index, diagrammer: &Diagrammer) -> Result<String> {
        let mut content = String::from("# デプロイ\n\n");

        content.push_str("## デプロイメント構成\n\n");

        // デプロイメント図を生成
        let diagram = diagrammer.generate_diagram(index, "deployment")?;
        if diagram.format == "mermaid" {
            content.push_str(&format!("```mermaid\n{}\n```\n\n", diagram.content));
        }

        content.push_str("## エントリーポイント\n\n");
        for ep in &index.entrypoints {
            content.push_str(&format!("- `{}`\n", ep.display()));
        }

        Ok(content)
    }

    /// FAQセクションを並列実行用に生成
    async fn generate_faq_parallel(index: &Index) -> Result<String> {
        let mut content = String::from("# FAQ\n\n");

        content.push_str("## よくある質問\n\n");
        content.push_str("### このリポジトリは何ですか？\n\n");
        content.push_str(&format!(
            "{}ファイル、{}言語、{}モジュールを含むリポジトリです。\n\n",
            index.stats.files,
            index.stats.languages.len(),
            index.stats.modules
        ));

        content.push_str("### どのように始めますか？\n\n");
        if !index.entrypoints.is_empty() {
            content.push_str("エントリーポイント:\n");
            for ep in &index.entrypoints {
                content.push_str(&format!("- `{}`\n", ep.display()));
            }
        } else {
            content.push_str("エントリーポイントが見つかりませんでした。\n");
        }

        Ok(content)
    }

    /// 概要セクションを生成
    async fn generate_overview(&self, index: &Index) -> Result<String> {
        let summary_result = self
            .summarizer
            .summarize(index, "repo", "", "concise-ja")
            .await?;

        Ok(summary_result.content_md)
    }

    /// アーキテクチャセクションを生成
    async fn generate_architecture(&self, index: &Index, with_diagrams: bool) -> Result<String> {
        let mut content = String::from("# アーキテクチャ\n\n");

        content.push_str("## システム構成\n\n");
        content.push_str(&format!(
            "このリポジトリは{}ファイル、{}言語、{}モジュールで構成されています。\n\n",
            index.stats.files,
            index.stats.languages.len(),
            index.stats.modules
        ));

        if with_diagrams {
            content.push_str("## モジュールグラフ\n\n");
            let diagram = self.diagrammer.generate_diagram(index, "module-graph")?;
            if diagram.format == "mermaid" {
                content.push_str(&format!("```mermaid\n{}\n```\n\n", diagram.content));
            }
        }

        content.push_str("## 主要コンポーネント\n\n");
        for module in &index.modules {
            content.push_str(&format!("- **{}** (`{}`)\n", module.name, module.path.display()));
        }

        Ok(content)
    }

    /// モジュールセクションを生成
    async fn generate_modules(&self, index: &Index) -> Result<String> {
        let mut content = String::from("# モジュール\n\n");

        for module in &index.modules {
            content.push_str(&format!("## {}\n\n", module.name));
            content.push_str(&format!("パス: `{}`\n\n", module.path.display()));
            content.push_str(&format!("言語: {}\n\n", module.language));

            if !module.dependencies.is_empty() {
                content.push_str("### 依存関係\n\n");
                for dep in &module.dependencies {
                    content.push_str(&format!("- `{}`\n", dep));
                }
                content.push_str("\n");
            }

            // モジュールの要約を生成
            let summary_result = self
                .summarizer
                .summarize(
                    index,
                    "module",
                    &module.path.to_string_lossy(),
                    "concise-ja",
                )
                .await?;
            content.push_str(&summary_result.content_md);
            content.push_str("\n\n");
        }

        Ok(content)
    }

    /// フローセクションを生成
    async fn generate_flows(&self, index: &Index, with_diagrams: bool) -> Result<String> {
        let mut content = String::from("# フロー\n\n");

        if with_diagrams {
            content.push_str("## シーケンス図\n\n");
            let diagram = self.diagrammer.generate_diagram(index, "sequence")?;
            if diagram.format == "mermaid" {
                content.push_str(&format!("```mermaid\n{}\n```\n\n", diagram.content));
            }

            content.push_str("## コールグラフ\n\n");
            let diagram = self.diagrammer.generate_diagram(index, "call-graph")?;
            if diagram.format == "mermaid" {
                content.push_str(&format!("```mermaid\n{}\n```\n\n", diagram.content));
            }
        }

        Ok(content)
    }

    /// デプロイセクションを生成
    async fn generate_deploy(&self, index: &Index) -> Result<String> {
        let mut content = String::from("# デプロイ\n\n");

        content.push_str("## デプロイメント構成\n\n");

        // デプロイメント図を生成
        let diagram = self.diagrammer.generate_diagram(index, "deployment")?;
        if diagram.format == "mermaid" {
            content.push_str(&format!("```mermaid\n{}\n```\n\n", diagram.content));
        }

        content.push_str("## エントリーポイント\n\n");
        for ep in &index.entrypoints {
            content.push_str(&format!("- `{}`\n", ep.display()));
        }

        Ok(content)
    }

    /// FAQセクションを生成
    async fn generate_faq(&self, index: &Index) -> Result<String> {
        let mut content = String::from("# FAQ\n\n");

        content.push_str("## よくある質問\n\n");
        content.push_str("### このリポジトリは何ですか？\n\n");
        content.push_str(&format!(
            "{}ファイル、{}言語、{}モジュールを含むリポジトリです。\n\n",
            index.stats.files,
            index.stats.languages.len(),
            index.stats.modules
        ));

        content.push_str("### どのように始めますか？\n\n");
        if !index.entrypoints.is_empty() {
            content.push_str("エントリーポイント:\n");
            for ep in &index.entrypoints {
                content.push_str(&format!("- `{}`\n", ep.display()));
            }
        } else {
            content.push_str("エントリーポイントが見つかりませんでした。\n");
        }

        Ok(content)
    }

    /// mdBookをビルド
    /// 
    /// # 引数
    /// * `out_dir` - 出力ディレクトリ
    /// 
    /// # 戻り値
    /// * `Result<()>` - 成功、またはエラー
    fn build_mdbook(&self, out_dir: &Path) -> Result<()> {
        info!("mdBookをビルド中...");

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

        info!("mdBookビルド完了");
        Ok(())
    }
}

/// Wikiビルド結果
#[derive(Debug, Serialize, Deserialize)]
pub struct WikiResult {
    pub ok: bool,
    pub site_dir: PathBuf,
    pub pages: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_section_name() {
        let config = Config::default();
        let builder = MdBookBuilder::new(config);
        assert_eq!(builder.get_section_name("overview"), "概要");
        assert_eq!(builder.get_section_name("architecture"), "アーキテクチャ");
    }
}

