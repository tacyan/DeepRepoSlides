/**
 * サマライザー実装
 * 
 * コードの要約を生成する
 * - 静的ヒューリスティックによる要約（LLMなし）
 * - LLMを使用した要約（オプション）
 * - 日本語フォーカスのプロンプト
 * 
 * 主な仕様:
 * - リポジトリ/パッケージ/モジュール/ファイル単位での要約
 * - concise-ja（簡潔）とdetailed-ja（詳細）の2スタイル
 * - アーティファクト（Mermaid図など）の生成
 * 
 * 制限事項:
 * - LLM統合は環境変数による設定が必要
 * - オフラインモードでは静的ヒューリスティックのみ使用
 */

use serde::{Deserialize, Serialize};
use std::path::Path;
use anyhow::Result;
use tracing::info;

use config::Config;
use analyzer_core::{Index, FileInfo};

/// サマライザー
pub struct Summarizer {
    #[allow(dead_code)]
    config: Config,
}

impl Summarizer {
    /// 新しいサマライザーインスタンスを作成
    /// 
    /// # 引数
    /// * `config` - 設定
    /// 
    /// # 戻り値
    /// * `Self` - サマライザーインスタンス
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// 要約を生成
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// * `scope` - スコープ（repo|package|module|file）
    /// * `target` - 対象（パスまたはモジュールID）
    /// * `style` - スタイル（concise-ja|detailed-ja）
    /// 
    /// # 戻り値
    /// * `Result<SummarizeResult>` - 要約結果、またはエラー
    pub async fn summarize(
        &self,
        index: &Index,
        scope: &str,
        target: &str,
        style: &str,
    ) -> Result<SummarizeResult> {
        info!("要約生成開始: scope={}, target={}, style={}", scope, target, style);

        let content_md = match scope {
            "repo" => self.summarize_repo(index, style).await?,
            "package" => self.summarize_package(index, target, style).await?,
            "module" => self.summarize_module(index, target, style).await?,
            "file" => self.summarize_file(index, target, style).await?,
            _ => return Err(anyhow::anyhow!("不明なスコープ: {}", scope)),
        };

        let artifacts = self.generate_artifacts(index, scope, target).await?;

        Ok(SummarizeResult {
            ok: true,
            content_md,
            artifacts,
        })
    }

    /// リポジトリ全体の要約を生成
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// * `style` - スタイル
    /// 
    /// # 戻り値
    /// * `Result<String>` - Markdown形式の要約、またはエラー
    async fn summarize_repo(&self, index: &Index, style: &str) -> Result<String> {
        let mut sections = Vec::new();

        // 概要
        sections.push(format!(
            "# {}\n\n{}ファイル、{}言語、{}モジュールを含むリポジトリです。\n",
            index.repo_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("リポジトリ"),
            index.stats.files,
            index.stats.languages.len(),
            index.stats.modules
        ));

        // 目的・コンポーネント
        if style == "detailed-ja" {
            sections.push("## 目的・コンポーネント\n\n".to_string());
            sections.push(self.infer_purpose(index).await);
            sections.push("\n".to_string());
        }

        // 主要データ・ユーザーフロー
        sections.push("## 主要コンポーネント\n\n".to_string());
        sections.push(self.describe_components(index).await);
        sections.push("\n".to_string());

        // 外部依存
        if !index.dependencies.is_empty() {
            sections.push("## 外部依存\n\n".to_string());
            sections.push(self.describe_dependencies(index).await);
            sections.push("\n".to_string());
        }

        // エントリーポイント
        if !index.entrypoints.is_empty() {
            sections.push("## エントリーポイント\n\n".to_string());
            for ep in &index.entrypoints {
                sections.push(format!("- `{}`\n", ep.display()));
            }
            sections.push("\n".to_string());
        }

        Ok(sections.join(""))
    }

    /// パッケージの要約を生成
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// * `target` - 対象パス
    /// * `style` - スタイル
    /// 
    /// # 戻り値
    /// * `Result<String>` - Markdown形式の要約、またはエラー
    async fn summarize_package(&self, index: &Index, target: &str, _style: &str) -> Result<String> {
        let target_path = Path::new(target);
        let package_files: Vec<&FileInfo> = index
            .files
            .iter()
            .filter(|f| f.path.starts_with(target_path))
            .collect();

        if package_files.is_empty() {
            return Err(anyhow::anyhow!("パッケージが見つかりません: {}", target));
        }

        let mut sections = Vec::new();
        sections.push(format!("# {}\n\n", target_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("パッケージ")));

        sections.push(format!("{}ファイルを含むパッケージです。\n\n", package_files.len()));

        // モジュール一覧
        sections.push("## モジュール\n\n".to_string());
        for file in &package_files {
            if file.is_module {
                sections.push(format!("- `{}`\n", file.path.display()));
            }
        }

        Ok(sections.join(""))
    }

    /// モジュールの要約を生成
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// * `target` - 対象パス
    /// * `style` - スタイル
    /// 
    /// # 戻り値
    /// * `Result<String>` - Markdown形式の要約、またはエラー
    async fn summarize_module(&self, index: &Index, target: &str, style: &str) -> Result<String> {
        let target_path = Path::new(target);
        let file_info = index
            .files
            .iter()
            .find(|f| f.path == target_path)
            .ok_or_else(|| anyhow::anyhow!("モジュールが見つかりません: {}", target))?;

        let mut sections = Vec::new();
        sections.push(format!("# {}\n\n", file_info.name));

        // 役割
        sections.push("## 役割\n\n".to_string());
        sections.push(self.infer_role(file_info).await);
        sections.push("\n".to_string());

        // 公開API（簡易的な推定）
        if !file_info.dependencies.is_empty() {
            sections.push("## 依存関係\n\n".to_string());
            for dep in &file_info.dependencies {
                sections.push(format!("- `{}`\n", dep));
            }
            sections.push("\n".to_string());
        }

        // 注意点（静的ヒューリスティック）
        if style == "detailed-ja" {
            sections.push("## 注意点\n\n".to_string());
            sections.push(self.infer_notes(file_info).await);
            sections.push("\n".to_string());
        }

        Ok(sections.join(""))
    }

    /// ファイルの要約を生成
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// * `target` - 対象パス
    /// * `style` - スタイル
    /// 
    /// # 戻り値
    /// * `Result<String>` - Markdown形式の要約、またはエラー
    async fn summarize_file(&self, index: &Index, target: &str, _style: &str) -> Result<String> {
        let target_path = Path::new(target);
        let file_info = index
            .files
            .iter()
            .find(|f| f.path == target_path)
            .ok_or_else(|| anyhow::anyhow!("ファイルが見つかりません: {}", target))?;

        let mut sections = Vec::new();
        sections.push(format!("# {}\n\n", file_info.name));

        if let Some(content) = &file_info.content {
            sections.push("## 概要\n\n".to_string());
            sections.push(self.summarize_content(content, &file_info.language).await);
            sections.push("\n".to_string());
        }

        Ok(sections.join(""))
    }

    /// 目的を推定
    async fn infer_purpose(&self, index: &Index) -> String {
        let mut purposes = Vec::new();

        // ファイル名から推測
        let main_files: Vec<&FileInfo> = index
            .files
            .iter()
            .filter(|f| f.name.contains("main") || f.name.contains("server") || f.name.contains("app"))
            .collect();

        if !main_files.is_empty() {
            purposes.push("アプリケーションまたはサーバーとして動作する可能性があります。".to_string());
        }

        // 依存関係から推測
        for (dep, _) in &index.dependencies {
            if dep.contains("express") || dep.contains("fastapi") || dep.contains("flask") {
                purposes.push("WebアプリケーションまたはAPIサーバーです。".to_string());
                break;
            }
            if dep.contains("react") || dep.contains("vue") || dep.contains("angular") {
                purposes.push("フロントエンドアプリケーションです。".to_string());
                break;
            }
        }

        if purposes.is_empty() {
            purposes.push("コードベースの目的を特定するには追加の分析が必要です。".to_string());
        }

        purposes.join("\n")
    }

    /// コンポーネントを記述
    async fn describe_components(&self, index: &Index) -> String {
        let mut descriptions = Vec::new();

        // モジュールごとに説明
        for module in &index.modules {
            descriptions.push(format!(
                "- **{}** (`{}`): {}言語で記述されたモジュール",
                module.name,
                module.path.display(),
                module.language
            ));
        }

        if descriptions.is_empty() {
            descriptions.push("コンポーネント情報がありません。".to_string());
        }

        descriptions.join("\n")
    }

    /// 依存関係を記述
    async fn describe_dependencies(&self, index: &Index) -> String {
        let mut deps_list = Vec::new();
        for (dep, _) in &index.dependencies {
            deps_list.push(format!("- `{}`", dep));
        }
        deps_list.join("\n")
    }

    /// 役割を推定
    async fn infer_role(&self, file_info: &FileInfo) -> String {
        let name_lower = file_info.name.to_lowercase();

        if name_lower.contains("config") || name_lower.contains("setting") {
            "設定管理を行うモジュールです。".to_string()
        } else if name_lower.contains("api") || name_lower.contains("route") {
            "APIエンドポイントまたはルーティングを定義するモジュールです。".to_string()
        } else if name_lower.contains("util") || name_lower.contains("helper") {
            "ユーティリティ関数を提供するモジュールです。".to_string()
        } else if name_lower.contains("model") || name_lower.contains("schema") {
            "データモデルまたはスキーマを定義するモジュールです。".to_string()
        } else if name_lower.contains("service") || name_lower.contains("business") {
            "ビジネスロジックを実装するモジュールです。".to_string()
        } else {
            format!("{}で記述されたモジュールです。", file_info.language)
        }
    }

    /// 注意点を推定
    async fn infer_notes(&self, file_info: &FileInfo) -> String {
        let mut notes = Vec::new();

        if file_info.size > 10000 {
            notes.push("ファイルサイズが大きいため、リファクタリングを検討してください。".to_string());
        }

        if file_info.dependencies.len() > 20 {
            notes.push("依存関係が多く、結合度が高い可能性があります。".to_string());
        }

        if notes.is_empty() {
            notes.push("特に注意すべき点は見つかりませんでした。".to_string());
        }

        notes.join("\n")
    }

    /// メソッド単位での詳細な解説を生成
    /// 
    /// # 引数
    /// * `content` - ファイル内容
    /// * `language` - 言語
    /// 
    /// # 戻り値
    /// * `Vec<MethodInfo>` - メソッド情報のリスト
    pub fn extract_methods_detailed(&self, content: &str, language: &str) -> Vec<MethodInfo> {
        let mut methods = Vec::new();

        match language {
            "rs" => {
                // Rust関数を詳細に抽出
                let func_re = regex::Regex::new(r"(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*\([^)]*\)\s*(?:->\s*[^{]+)?\s*\{").unwrap();
                for cap in func_re.captures_iter(content) {
                    if let Some(name) = cap.get(1) {
                        let func_name = name.as_str();
                        // 関数の前のコメントを探す
                        let lines: Vec<&str> = content.lines().collect();
                        let mut doc = String::new();
                        for (i, line) in lines.iter().enumerate() {
                            if line.contains(&format!("fn {}", func_name)) {
                                // 前の行のコメントを収集
                                let mut j = i.saturating_sub(1);
                                while j > 0 && (lines[j].trim_start().starts_with("///") || lines[j].trim().is_empty()) {
                                    if lines[j].trim_start().starts_with("///") {
                                        doc.push_str(&lines[j].trim_start().trim_start_matches("///").trim());
                                        doc.push_str("\n");
                                    }
                                    j = j.saturating_sub(1);
                                }
                                break;
                            }
                        }
                        
                        methods.push(MethodInfo {
                            name: func_name.to_string(),
                            language: language.to_string(),
                            documentation: doc.trim().to_string(),
                            code_snippet: self.extract_method_code(content, func_name, language),
                        });
                    }
                }
            }
            "ts" | "js" => {
                // JavaScript/TypeScript関数を詳細に抽出
                let func_re = regex::Regex::new(r"(?:export\s+)?(?:async\s+)?function\s+(\w+)\s*\([^)]*\)\s*(?::\s*[^{]+)?\s*\{").unwrap();
                for cap in func_re.captures_iter(content) {
                    if let Some(name) = cap.get(1) {
                        let func_name = name.as_str();
                        let doc = self.infer_function_purpose_simple(func_name);
                        methods.push(MethodInfo {
                            name: func_name.to_string(),
                            language: language.to_string(),
                            documentation: doc,
                            code_snippet: self.extract_method_code(content, func_name, language),
                        });
                    }
                }
            }
            "py" => {
                // Python関数を詳細に抽出
                let func_re = regex::Regex::new(r"def\s+(\w+)\s*\([^)]*\)\s*:").unwrap();
                for cap in func_re.captures_iter(content) {
                    if let Some(name) = cap.get(1) {
                        let func_name = name.as_str();
                        let doc = self.infer_function_purpose_simple(func_name);
                        methods.push(MethodInfo {
                            name: func_name.to_string(),
                            language: language.to_string(),
                            documentation: doc,
                            code_snippet: self.extract_method_code(content, func_name, language),
                        });
                    }
                }
            }
            _ => {}
        }

        methods
    }

    /// メソッドのコードスニペットを抽出
    fn extract_method_code(&self, content: &str, method_name: &str, language: &str) -> String {
        let lines: Vec<&str> = content.lines().collect();
        let mut in_method = false;
        let mut brace_count = 0;
        let mut method_lines = Vec::new();
        let search_pattern = match language {
            "rs" => format!("fn {}", method_name),
            "ts" | "js" => format!("function {}", method_name),
            "py" => format!("def {}", method_name),
            _ => method_name.to_string(),
        };

        for line in &lines {
            if line.contains(&search_pattern) {
                in_method = true;
                brace_count = 0;
            }
            
            if in_method {
                method_lines.push(*line);
                
                // ブレースのカウント（Rust/JS/TS）
                if language == "rs" || language == "ts" || language == "js" {
                    brace_count += line.matches('{').count();
                    brace_count -= line.matches('}').count();
                    if brace_count == 0 && method_lines.len() > 1 {
                        break;
                    }
                } else if language == "py" {
                    // Pythonの場合はインデントで判定
                    if method_lines.len() > 1 {
                        let first_indent = method_lines[0].len() - method_lines[0].trim_start().len();
                        let current_indent = line.len() - line.trim_start().len();
                        if current_indent <= first_indent && !line.trim().is_empty() {
                            method_lines.pop();
                            break;
                        }
                    }
                }
            }
        }

        method_lines.join("\n")
    }

    /// 関数の目的を推定（簡易版）
    fn infer_function_purpose_simple(&self, func_name: &str) -> String {
        let name_lower = func_name.to_lowercase();
        
        // 関数名から目的を推定
        if name_lower.contains("get") || name_lower.contains("fetch") {
            "データを取得する関数です。".to_string()
        } else if name_lower.contains("set") || name_lower.contains("update") {
            "データを設定または更新する関数です。".to_string()
        } else if name_lower.contains("create") || name_lower.contains("make") {
            "新しいオブジェクトやデータを作成する関数です。".to_string()
        } else if name_lower.contains("delete") || name_lower.contains("remove") {
            "データを削除する関数です。".to_string()
        } else if name_lower.contains("parse") || name_lower.contains("convert") {
            "データを変換または解析する関数です。".to_string()
        } else if name_lower.contains("validate") || name_lower.contains("check") {
            "データを検証またはチェックする関数です。".to_string()
        } else if name_lower.contains("handle") || name_lower.contains("process") {
            "イベントやデータを処理する関数です。".to_string()
        } else {
            format!("`{}`関数の実装です。", func_name)
        }
    }

    /// コンテンツを要約（メソッド単位での詳細な解説を含む）
    async fn summarize_content(&self, content: &str, language: &str) -> String {
        let mut summary = String::new();
        
        // メソッド単位での詳細な解説を生成
        let methods = self.extract_methods_detailed(content, language);
        
        if !methods.is_empty() {
            summary.push_str("## 主要な関数・メソッド\n\n");
            for method in methods.iter().take(10) {
                summary.push_str(&format!("### {}\n\n", method.name));
                
                if !method.documentation.is_empty() {
                    summary.push_str(&format!("**説明**: {}\n\n", method.documentation));
                }
                
                // コードスニペットを追加（短い場合のみ）
                let code_lines: Vec<&str> = method.code_snippet.lines().collect();
                if code_lines.len() <= 20 {
                    summary.push_str("```");
                    summary.push_str(&method.language);
                    summary.push_str("\n");
                    summary.push_str(&method.code_snippet);
                    summary.push_str("\n```\n\n");
                } else {
                    summary.push_str("```");
                    summary.push_str(&method.language);
                    summary.push_str("\n");
                    // 最初の10行と最後の5行を表示
                    for line in code_lines.iter().take(10) {
                        summary.push_str(line);
                        summary.push_str("\n");
                    }
                    summary.push_str("// ... (省略) ...\n");
                    for line in code_lines.iter().skip(code_lines.len().saturating_sub(5)) {
                        summary.push_str(line);
                        summary.push_str("\n");
                    }
                    summary.push_str("```\n\n");
                }
            }
        } else {
            // 簡易的な要約（関数/クラス名を抽出）
            match language {
                "ts" | "js" => {
                    let func_re = regex::Regex::new(r"(?:export\s+)?(?:async\s+)?function\s+(\w+)").unwrap();
                    let funcs: Vec<&str> = func_re
                        .captures_iter(content)
                        .filter_map(|cap| cap.get(1))
                        .map(|m| m.as_str())
                        .collect();
                    if !funcs.is_empty() {
                        summary.push_str("主要な関数:\n");
                        for func in funcs {
                            summary.push_str(&format!("- `{}`\n", func));
                        }
                    }
                }
                "py" => {
                    let func_re = regex::Regex::new(r"def\s+(\w+)").unwrap();
                    let funcs: Vec<&str> = func_re
                        .captures_iter(content)
                        .filter_map(|cap| cap.get(1))
                        .map(|m| m.as_str())
                        .collect();
                    if !funcs.is_empty() {
                        summary.push_str("主要な関数:\n");
                        for func in funcs {
                            summary.push_str(&format!("- `{}`\n", func));
                        }
                    }
                }
                "rs" => {
                    let func_re = regex::Regex::new(r"fn\s+(\w+)").unwrap();
                    let funcs: Vec<&str> = func_re
                        .captures_iter(content)
                        .filter_map(|cap| cap.get(1))
                        .map(|m| m.as_str())
                        .collect();
                    if !funcs.is_empty() {
                        summary.push_str("主要な関数:\n");
                        for func in funcs {
                            summary.push_str(&format!("- `{}`\n", func));
                        }
                    }
                }
                _ => {
                    summary.push_str("コードの要約を生成しました。");
                }
            }
        }

        if summary.is_empty() {
            summary.push_str(&format!("{}行のコードを含むファイルです。", content.lines().count()));
        }

        summary
    }

    /// アーティファクトを生成
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// * `scope` - スコープ
    /// * `target` - 対象
    /// 
    /// # 戻り値
    /// * `Result<Vec<Artifact>>` - アーティファクトのリスト、またはエラー
    async fn generate_artifacts(
        &self,
        index: &Index,
        scope: &str,
        _target: &str,
    ) -> Result<Vec<Artifact>> {
        let mut artifacts = Vec::new();

        // モジュールグラフの生成（簡易版）
        if scope == "repo" || scope == "package" {
            let mermaid_content = self.generate_module_graph_mermaid(index).await?;
            artifacts.push(Artifact {
                artifact_type: "mermaid".to_string(),
                path: format!("./out/diagrams/module-graph-{}.mmd", scope),
                content: mermaid_content,
            });
        }

        Ok(artifacts)
    }

    /// モジュールグラフのMermaid DSLを生成
    async fn generate_module_graph_mermaid(&self, index: &Index) -> Result<String> {
        let mut mermaid = String::from("graph TD\n");
        let mut node_count = 0;

        for module in &index.modules {
            let node_id = format!("M{}", node_count);
            let label = module.name.clone();
            mermaid.push_str(&format!("    {}[\"{}\"]\n", node_id, label));
            node_count += 1;
        }

        Ok(mermaid)
    }
}

/// メソッド情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodInfo {
    pub name: String,
    pub language: String,
    pub documentation: String,
    pub code_snippet: String,
}

/// 要約結果
#[derive(Debug, Serialize, Deserialize)]
pub struct SummarizeResult {
    pub ok: bool,
    pub content_md: String,
    pub artifacts: Vec<Artifact>,
}

/// アーティファクト
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub artifact_type: String,
    pub path: String,
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_role() {
        let config = Config::default();
        let summarizer = Summarizer::new(config);
        let file_info = analyzer_core::FileInfo {
            path: Path::new("config.ts").to_path_buf(),
            name: "config".to_string(),
            language: "ts".to_string(),
            size: 1000,
            dependencies: vec![],
            is_module: true,
            content: None,
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let role = rt.block_on(summarizer.infer_role(&file_info));
        assert!(role.contains("設定"));
    }
}

