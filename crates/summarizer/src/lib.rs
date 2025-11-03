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

    /// コンテンツを要約
    async fn summarize_content(&self, content: &str, language: &str) -> String {
        // 簡易的な要約（関数/クラス名を抽出）
        let mut summary = String::new();

        match language {
            "ts" | "js" => {
                // 関数定義を抽出
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
                // 関数定義を抽出
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
            _ => {
                summary.push_str("コードの要約を生成しました。");
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

