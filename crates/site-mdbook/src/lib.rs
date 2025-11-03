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

        // 各章を並列生成（50並列対応：tech-book-readerの実装を参考）
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
        
        // モジュールページは50並列で1つのファイルにまとめて生成
        if toc.contains(&"modules".to_string()) {
            let index_for_modules = index_clone.clone();
            let config_for_modules = config_clone.clone();
            
            // まず、モジュール一覧を生成
            let mut modules_content = String::from("# モジュール\n\n");
            modules_content.push_str("このセクションでは、各モジュールについて詳しく説明します。\n\n");
            modules_content.push_str("## モジュール一覧\n\n");
            for module in &index.modules {
                // mdBookのアンカーリンクは見出しから自動生成されるため、見出しテキストをそのまま使用
                // 特殊文字はmdBookが自動的に処理するので、そのまま使用
                modules_content.push_str(&format!("- [{}](#{})\n", module.name, module.name));
            }
            modules_content.push_str("\n\n---\n\n");
            
            // 各モジュールごとに50並列で処理して、1つのファイルにまとめる
            let mut module_handles = Vec::new();
            let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(50));
            
            for module in &index.modules {
                let module = module.clone();
                let index_for_module = index_for_modules.clone();
                let config_for_module = config_for_modules.clone();
                let permit = semaphore.clone();
                
                let handle = tokio::spawn(async move {
                    let _permit = permit.acquire().await.unwrap();
                    let summarizer = Summarizer::new(config_for_module.clone());
                    
                    Self::generate_module_content_detailed(
                        &index_for_module,
                        &module,
                        &summarizer,
                    ).await
                });
                module_handles.push(handle);
            }
            
            // すべてのモジュールページを並列実行して結果を収集
            for handle in module_handles {
                if let Ok(Ok(module_content)) = handle.await {
                    modules_content.push_str(&module_content);
                    modules_content.push_str("\n\n---\n\n");
                }
            }
            
            // 1つのファイルにまとめる
            let modules_file_path = src_dir.join("modules.md");
            fs::write(&modules_file_path, modules_content)
                .with_context(|| format!("modules.mdの書き込みに失敗しました: {:?}", modules_file_path))?;
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

    /// モジュールコンテンツを詳細に生成（50並列対応、文字列を返す）
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// * `module` - モジュール情報
    /// * `summarizer` - サマライザー
    /// 
    /// # 戻り値
    /// * `Result<String>` - モジュールコンテンツ、またはエラー
    async fn generate_module_content_detailed(
        index: &Index,
        module: &analyzer_core::ModuleInfo,
        summarizer: &Summarizer,
    ) -> Result<String> {
        let mut content = String::new();
        
        // mdBookのアンカーリンクは見出しから自動生成されるため、見出しをそのまま使用
        content.push_str(&format!("## {}\n\n", module.name));
        content.push_str(&format!("**ファイル**: `{}`  \n", module.path.display()));
        content.push_str(&format!("**言語**: {}\n\n", module.language));
        
        // ファイル情報を取得してメソッドを抽出
        if let Some(file_info) = index.files.iter().find(|f| f.path == module.path) {
            if let Some(file_content) = &file_info.content {
                let methods = summarizer.extract_methods_detailed(file_content, &file_info.language);
                
                if !methods.is_empty() {
                    content.push_str("### 主要な関数・メソッド\n\n");
                    content.push_str("このモジュールには以下の関数やメソッドが含まれています。各メソッドについて、日本語で詳しく解説します。\n\n");
                    
                    // 各メソッドごとに詳細な解説を生成
                    for method in methods.iter().take(30) {
                        content.push_str(&format!("#### {}\n\n", method.name));
                        
                        // 日本語の説明を生成（英語コメントを翻訳）
                        let doc_ja = if !method.documentation.is_empty() {
                            summarizer.translate_doc_to_japanese(&method.documentation)
                        } else {
                            summarizer.infer_function_purpose_simple(&method.name)
                        };
                        
                        content.push_str(&format!("{}\n\n", doc_ja));
                        
                        // コードの動作を詳しく説明
                        content.push_str("##### コードの動作\n\n");
                        content.push_str("この関数の実装を見てみましょう。\n\n");
                        
                        // コードブロック（必ず表示）
                        let code_lines: Vec<&str> = method.code_snippet.lines().collect();
                        if code_lines.len() <= 40 {
                            content.push_str("```");
                            content.push_str(&method.language);
                            content.push_str("\n");
                            content.push_str(&method.code_snippet);
                            content.push_str("\n```\n\n");
                        } else {
                            // 重要な部分だけ表示
                            content.push_str("```");
                            content.push_str(&method.language);
                            content.push_str("\n");
                            for line in code_lines.iter().take(20) {
                                content.push_str(line);
                                content.push_str("\n");
                            }
                            content.push_str("// ... (省略) ...\n");
                            for line in code_lines.iter().skip(code_lines.len().saturating_sub(5)) {
                                content.push_str(line);
                                content.push_str("\n");
                            }
                            content.push_str("```\n\n");
                        }
                        
                        // コードの説明を追加
                        content.push_str("このコードは以下の処理を行います：\n\n");
                        content.push_str(&format!("- `{}`関数は、", method.name));
                        // コードから処理内容を推測
                        let code_lower = method.code_snippet.to_lowercase();
                        if code_lower.contains("return") {
                            content.push_str("値を返します。");
                        } else if code_lower.contains("mut") || code_lower.contains("let") {
                            content.push_str("変数を操作します。");
                        } else if code_lower.contains("if") || code_lower.contains("match") {
                            content.push_str("条件分岐を行います。");
                        } else if code_lower.contains("loop") || code_lower.contains("for") || code_lower.contains("while") {
                            content.push_str("繰り返し処理を行います。");
                        } else {
                            content.push_str("何らかの処理を実行します。");
                        }
                        content.push_str("\n\n");
                    }
                }
            }
        }
        
        Ok(content)
    }

    /// 個別のモジュールページを詳細に生成（50並列対応）
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// * `modules_dir` - モジュールディレクトリ
    /// * `module` - モジュール情報
    /// * `summarizer` - サマライザー
    /// 
    /// # 戻り値
    /// * `Result<()>` - 成功、またはエラー
    async fn generate_module_page_detailed(
        index: &Index,
        modules_dir: &Path,
        module: &analyzer_core::ModuleInfo,
        summarizer: &Summarizer,
    ) -> Result<()> {
        let mut content = String::new();
        
        content.push_str(&format!("# {}\n\n", module.name));
        content.push_str(&format!("**ファイル**: `{}`  \n", module.path.display()));
        content.push_str(&format!("**言語**: {}\n\n", module.language));
        
        // ファイル情報を取得してメソッドを抽出
        if let Some(file_info) = index.files.iter().find(|f| f.path == module.path) {
            if let Some(file_content) = &file_info.content {
                let methods = summarizer.extract_methods_detailed(file_content, &file_info.language);
                
                if !methods.is_empty() {
                    content.push_str("## 主要な関数・メソッド\n\n");
                    content.push_str("このモジュールには以下の関数やメソッドが含まれています。各メソッドについて、日本語で詳しく解説します。\n\n");
                    
                    // 各メソッドごとに詳細な解説を生成
                    for method in methods.iter().take(30) {
                        content.push_str(&format!("### {}\n\n", method.name));
                        
                        // 日本語の説明を生成（英語コメントを翻訳）
                        let doc_ja = if !method.documentation.is_empty() {
                            summarizer.translate_doc_to_japanese(&method.documentation)
                        } else {
                            summarizer.infer_function_purpose_simple(&method.name)
                        };
                        
                        content.push_str(&format!("{}\n\n", doc_ja));
                        
                        // コードの動作を詳しく説明
                        content.push_str("#### コードの動作\n\n");
                        content.push_str("この関数の実装を見てみましょう。\n\n");
                        
                        // コードブロック（必ず表示）
                        let code_lines: Vec<&str> = method.code_snippet.lines().collect();
                        if code_lines.len() <= 40 {
                            content.push_str("```");
                            content.push_str(&method.language);
                            content.push_str("\n");
                            content.push_str(&method.code_snippet);
                            content.push_str("\n```\n\n");
                        } else {
                            // 重要な部分だけ表示
                            content.push_str("```");
                            content.push_str(&method.language);
                            content.push_str("\n");
                            for line in code_lines.iter().take(20) {
                                content.push_str(line);
                                content.push_str("\n");
                            }
                            content.push_str("// ... (省略) ...\n");
                            for line in code_lines.iter().skip(code_lines.len().saturating_sub(5)) {
                                content.push_str(line);
                                content.push_str("\n");
                            }
                            content.push_str("```\n\n");
                        }
                        
                        // コードの説明を追加
                        content.push_str("このコードは以下の処理を行います：\n\n");
                        content.push_str(&format!("- `{}`関数は、", method.name));
                        // コードから処理内容を推測
                        let code_lower = method.code_snippet.to_lowercase();
                        if code_lower.contains("return") {
                            content.push_str("値を返します。");
                        } else if code_lower.contains("mut") || code_lower.contains("let") {
                            content.push_str("変数を操作します。");
                        } else if code_lower.contains("if") || code_lower.contains("match") {
                            content.push_str("条件分岐を行います。");
                        } else if code_lower.contains("loop") || code_lower.contains("for") || code_lower.contains("while") {
                            content.push_str("繰り返し処理を行います。");
                        } else {
                            content.push_str("何らかの処理を実行します。");
                        }
                        content.push_str("\n\n");
                    }
                }
            }
        }
        
        // ファイル名を安全な形に変換
        let safe_name = module.name.replace("::", "_").replace("/", "_").replace("\\", "_");
        let file_path = modules_dir.join(format!("{}.md", safe_name));
        fs::write(&file_path, content)
            .with_context(|| format!("モジュールページの書き込みに失敗しました: {:?}", file_path))?;
        
        Ok(())
    }
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
        // modulesセクションは別途50並列で生成されるため、ここではスキップ
        if section == "modules" {
            return Ok(1);
        }
        
        let content = match section {
            "overview" => Self::generate_overview_parallel(index, summarizer).await?,
            "architecture" => Self::generate_architecture_parallel(index, with_diagrams, diagrammer).await?,
            "flows" => Self::generate_flows_parallel(index, with_diagrams, diagrammer).await?,
            "deploy" => Self::generate_deploy_parallel(index, diagrammer).await?,
            "faq" => Self::generate_faq_parallel(index).await?,
            _ => format!("# {}\n\nセクションの内容\n", section),
        };

        let page_count = 1;

        let file_path = src_dir.join(format!("{}.md", section));
        fs::write(&file_path, content)
            .with_context(|| format!("セクションファイルの書き込みに失敗しました: {:?}", file_path))?;

        Ok(page_count)
    }

    /// セクションを生成（非並列実行用、後方互換性のため保持）
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// * `src_dir` - ソースディレクトリ
    /// * `section` - セクション名
    /// * `with_diagrams` - 図を含めるか
    /// 
    /// # 戻り値
    /// * `Result<usize>` - 生成されたページ数、またはエラー
    #[allow(dead_code)] // 後方互換性のため保持
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

    /// 概要セクションを並列実行用に生成（図のみ）
    async fn generate_overview_parallel(index: &Index, _summarizer: &Summarizer) -> Result<String> {
        let mut content = String::from("# 概要\n\n");
        content.push_str("このページでは、リポジトリ全体の構成を図で示します。\n\n");
        
        // 統計情報を簡潔に表示
        content.push_str("## 統計情報\n\n");
        content.push_str(&format!("- **ファイル数**: {}個\n", index.stats.files));
        content.push_str(&format!("- **使用言語**: {}\n", index.stats.languages.join(", ")));
        content.push_str(&format!("- **モジュール数**: {}個\n\n", index.stats.modules));
        
        // 全体構成図のみ
        content.push_str("## 全体構成図\n\n");
        content.push_str("```mermaid\n");
        content.push_str("graph TD\n");
        content.push_str("    A[リポジトリ全体] --> B[ファイル]\n");
        content.push_str("    A --> C[モジュール]\n");
        content.push_str("    A --> D[依存関係]\n");
        content.push_str(&format!("    B --> E[{}ファイル]\n", index.stats.files));
        content.push_str(&format!("    C --> F[{}モジュール]\n", index.stats.modules));
        content.push_str("```\n\n");

        Ok(content)
    }

    /// アーキテクチャセクションを並列実行用に生成（図のみ）
    async fn generate_architecture_parallel(
        index: &Index,
        with_diagrams: bool,
        diagrammer: &Diagrammer,
    ) -> Result<String> {
        let mut content = String::from("# アーキテクチャ\n\n");

        if with_diagrams {
            content.push_str("## モジュールグラフ\n\n");
            let diagram = diagrammer.generate_diagram(index, "module-graph")?;
            if diagram.format == "mermaid" {
                content.push_str(&format!("```mermaid\n{}\n```\n\n", diagram.content));
            }
        }

        Ok(content)
    }

    /// モジュールセクションを並列実行用に生成（実際のコンテンツは別途50並列で生成）
    async fn generate_modules_parallel(index: &Index, _summarizer: &Summarizer) -> Result<String> {
        let mut content = String::from("# モジュール\n\n");
        content.push_str("このセクションでは、各モジュールについて詳しく説明します。\n\n");
        content.push_str("各モジュールの詳細は以下の通りです。\n\n");

        // モジュール一覧を追加（アンカーリンク用）
        content.push_str("## モジュール一覧\n\n");
        for module in &index.modules {
            let anchor = module.name.replace("::", "_").replace("/", "_").replace("\\", "_").replace(" ", "_");
            content.push_str(&format!("- [{}](#{})\n", module.name, anchor));
        }
        content.push_str("\n");

        Ok(content)
    }

    /// フローセクションを並列実行用に生成（図のみ）
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

    /// デプロイセクションを並列実行用に生成（図のみ）
    async fn generate_deploy_parallel(index: &Index, diagrammer: &Diagrammer) -> Result<String> {
        let mut content = String::from("# デプロイ\n\n");

        content.push_str("## デプロイメント構成図\n\n");
        let diagram = diagrammer.generate_diagram(index, "deployment")?;
        if diagram.format == "mermaid" {
            content.push_str(&format!("```mermaid\n{}\n```\n\n", diagram.content));
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

    /// 概要セクションを生成（非並列実行用、後方互換性のため保持）
    #[allow(dead_code)] // 後方互換性のため保持
    async fn generate_overview(&self, index: &Index) -> Result<String> {
        let summary_result = self
            .summarizer
            .summarize(index, "repo", "", "concise-ja")
            .await?;

        Ok(summary_result.content_md)
    }

    /// アーキテクチャセクションを生成（非並列実行用、後方互換性のため保持）
    #[allow(dead_code)] // 後方互換性のため保持
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

    /// モジュールセクションを生成（非並列実行用、後方互換性のため保持）
    #[allow(dead_code)] // 後方互換性のため保持
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

    /// フローセクションを生成（非並列実行用、後方互換性のため保持）
    #[allow(dead_code)] // 後方互換性のため保持
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

    /// デプロイセクションを生成（非並列実行用、後方互換性のため保持）
    #[allow(dead_code)] // 後方互換性のため保持
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

    /// FAQセクションを生成（非並列実行用、後方互換性のため保持）
    #[allow(dead_code)] // 後方互換性のため保持
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

