/**
 * アナライザーコア実装
 * 
 * 多言語対応のコード解析を行う
 * - tree-sitterによる構文解析
 * - 依存関係の抽出
 * - エントリーポイントの推定
 * - モジュール構造の解析
 * 
 * 主な仕様:
 * - TypeScript/JavaScript, Python, Go, Rust, Javaに対応
 * - 言語ごとの特性に応じた解析ロジック
 * - インデックス形式でのデータ保存
 * 
 * 制限事項:
 * - tree-sitterのバインディングは外部で提供されることを想定
 * - 大規模ファイルはスキップ（設定で制御可能）
 */

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use tracing::{info, warn};
use walkdir::WalkDir;
use regex::Regex;

use config::Config;

/// アナライザー
pub struct Analyzer {
    #[allow(dead_code)]
    config: Config,
}

impl Analyzer {
    /// 新しいアナライザーインスタンスを作成
    /// 
    /// # 引数
    /// * `config` - 設定
    /// 
    /// # 戻り値
    /// * `Self` - アナライザーインスタンス
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// リポジトリを解析してインデックスを作成
    /// 
    /// # 引数
    /// * `repo_path` - リポジトリのパス
    /// * `config` - 設定（上書き用）
    /// 
    /// # 戻り値
    /// * `Result<Index>` - 作成されたインデックス、またはエラー
    pub async fn analyze_repo<P: AsRef<Path>>(
        &self,
        repo_path: P,
        config: &Config,
    ) -> Result<Index> {
        let repo_path = repo_path.as_ref();
        info!("リポジトリ解析開始: {:?}", repo_path);

        let mut files = Vec::new();
        let mut modules = Vec::new();
        let mut dependencies = HashMap::new();
        let mut languages = std::collections::HashSet::new();

        // ファイルを走査
        for entry in WalkDir::new(repo_path) {
            let entry = entry?;
            let path = entry.path();

            if !entry.file_type().is_file() {
                continue;
            }

            // 除外パターンのチェック
            if self.should_exclude(path, &config.project.exclude) {
                continue;
            }

            // ファイルサイズチェック
            let metadata = std::fs::metadata(path)?;
            let size_kb = metadata.len() / 1024;
            if size_kb > config.analysis.max_file_kb as u64 {
                warn!("ファイルが大きすぎるためスキップ: {:?} ({}KB)", path, size_kb);
                continue;
            }

            // 言語検出
            if let Some(lang) = self.detect_language(path) {
                languages.insert(lang.clone());

                match self.analyze_file(path, &lang).await {
                    Ok(file_info) => {
                        files.push(file_info.clone());
                        if file_info.is_module {
                            modules.push(ModuleInfo {
                                path: path.to_path_buf(),
                                name: file_info.name.clone(),
                                language: lang.clone(),
                                dependencies: file_info.dependencies.clone(),
                            });
                        }
                        // 依存関係をマップに追加
                        for dep in &file_info.dependencies {
                            dependencies.entry(dep.clone()).or_insert_with(Vec::new);
                        }
                    }
                    Err(e) => {
                        warn!("ファイル解析エラー: {:?} - {}", path, e);
                    }
                }
            }
        }

        info!(
            "リポジトリ解析完了: {}ファイル, {}言語, {}モジュール",
            files.len(),
            languages.len(),
            modules.len()
        );

        let stats = IndexStats {
            files: files.len(),
            languages: languages.iter().cloned().collect(),
            modules: modules.len(),
        };

        Ok(Index {
            id: uuid::Uuid::new_v4().to_string(),
            repo_path: repo_path.to_path_buf(),
            files,
            modules,
            languages: languages.into_iter().collect(),
            dependencies,
            entrypoints: self.infer_entrypoints(repo_path, &config)?,
            stats,
        })
    }

    /// ファイルを解析
    /// 
    /// # 引数
    /// * `path` - ファイルパス
    /// * `language` - 言語識別子
    /// 
    /// # 戻り値
    /// * `Result<FileInfo>` - ファイル情報、またはエラー
    async fn analyze_file(&self, path: &Path, language: &str) -> Result<FileInfo> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("ファイル読み込みエラー: {:?}", path))?;

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let dependencies = match language {
            "ts" | "js" | "tsx" | "jsx" => self.extract_js_dependencies(&content),
            "py" => self.extract_py_dependencies(&content),
            "go" => self.extract_go_dependencies(&content),
            "rs" => self.extract_rust_dependencies(&content),
            _ => Vec::new(),
        };

        let is_module = self.is_module_file(path, language);

        Ok(FileInfo {
            path: path.to_path_buf(),
            name,
            language: language.to_string(),
            size: content.len(),
            dependencies,
            is_module,
            content: Some(content),
        })
    }

    /// 言語を検出
    /// 
    /// # 引数
    /// * `path` - ファイルパス
    /// 
    /// # 戻り値
    /// * `Option<String>` - 言語識別子（対応していない場合はNone）
    fn detect_language(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;

        match ext {
            "ts" | "tsx" => Some("ts".to_string()),
            "js" | "jsx" | "mjs" | "cjs" => Some("js".to_string()),
            "py" => Some("py".to_string()),
            "go" => Some("go".to_string()),
            "rs" => Some("rs".to_string()),
            "java" => Some("java".to_string()),
            _ => None,
        }
    }

    /// JavaScript/TypeScriptの依存関係を抽出
    /// 
    /// # 引数
    /// * `content` - ファイル内容
    /// 
    /// # 戻り値
    /// * `Vec<String>` - 依存関係のリスト
    fn extract_js_dependencies(&self, content: &str) -> Vec<String> {
        let mut deps = Vec::new();

        // import文の抽出
        let import_re = Regex::new(r#"(?:import|export).*from\s+['"]([^'"]+)['"]"#).unwrap();
        for cap in import_re.captures_iter(content) {
            if let Some(dep) = cap.get(1) {
                deps.push(dep.as_str().to_string());
            }
        }

        // require文の抽出
        let require_re = Regex::new(r#"require\s*\(\s*['"]([^'"]+)['"]"#).unwrap();
        for cap in require_re.captures_iter(content) {
            if let Some(dep) = cap.get(1) {
                deps.push(dep.as_str().to_string());
            }
        }

        deps
    }

    /// Pythonの依存関係を抽出
    /// 
    /// # 引数
    /// * `content` - ファイル内容
    /// 
    /// # 戻り値
    /// * `Vec<String>` - 依存関係のリスト
    fn extract_py_dependencies(&self, content: &str) -> Vec<String> {
        let mut deps = Vec::new();

        // import文の抽出
        let import_re = Regex::new(r#"^(?:import|from)\s+([^\s]+)"#).unwrap();
        for line in content.lines() {
            if let Some(cap) = import_re.captures(line) {
                if let Some(dep) = cap.get(1) {
                    deps.push(dep.as_str().to_string());
                }
            }
        }

        deps
    }

    /// Goの依存関係を抽出
    /// 
    /// # 引数
    /// * `content` - ファイル内容
    /// 
    /// # 戻り値
    /// * `Vec<String>` - 依存関係のリスト
    fn extract_go_dependencies(&self, content: &str) -> Vec<String> {
        let mut deps = Vec::new();

        // import文の抽出
        let import_re = Regex::new(r#"import\s+(?:\(([^)]+)\)|["']([^"']+)["'])"#).unwrap();
        for cap in import_re.captures_iter(content) {
            if let Some(dep) = cap.get(2) {
                deps.push(dep.as_str().to_string());
            } else if let Some(block) = cap.get(1) {
                // 複数行import
                for line in block.as_str().lines() {
                    let line_re = Regex::new(r#"["']([^"']+)["']"#).unwrap();
                    for line_cap in line_re.captures_iter(line) {
                        if let Some(dep) = line_cap.get(1) {
                            deps.push(dep.as_str().to_string());
                        }
                    }
                }
            }
        }

        deps
    }

    /// Rustの依存関係を抽出
    /// 
    /// # 引数
    /// * `content` - ファイル内容
    /// 
    /// # 戻り値
    /// * `Vec<String>` - 依存関係のリスト
    fn extract_rust_dependencies(&self, content: &str) -> Vec<String> {
        let mut deps = Vec::new();

        // use文の抽出
        let use_re = Regex::new(r#"use\s+([^;]+);"#).unwrap();
        for cap in use_re.captures_iter(content) {
            if let Some(use_stmt) = cap.get(1) {
                let path = use_stmt.as_str().trim();
                // エイリアスや特定のアイテムを除外
                if !path.contains("::") && !path.contains("{") {
                    continue;
                }
                let parts: Vec<&str> = path.split("::").collect();
                if let Some(first) = parts.first() {
                    deps.push(first.trim().to_string());
                }
            }
        }

        deps
    }

    /// モジュールファイルかどうかを判定
    /// 
    /// # 引数
    /// * `path` - ファイルパス
    /// * `language` - 言語識別子
    /// 
    /// # 戻り値
    /// * `bool` - モジュールファイルの場合true
    fn is_module_file(&self, path: &Path, language: &str) -> bool {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let parent = path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str());

        match language {
            "ts" | "js" => {
                // index.ts, index.js, または特定のディレクトリ構造
                file_name == "index.ts"
                    || file_name == "index.js"
                    || file_name == "index.tsx"
                    || file_name == "index.jsx"
                    || parent == Some("src")
                    || parent == Some("lib")
            }
            "py" => {
                file_name == "__init__.py" || parent == Some("src") || parent == Some("lib")
            }
            "go" => {
                // package main または特定のディレクトリ構造
                parent == Some("cmd") || parent == Some("pkg")
            }
            "rs" => {
                // lib.rs または特定のディレクトリ構造
                file_name == "lib.rs" || parent == Some("src")
            }
            _ => false,
        }
    }

    /// エントリーポイントを推定
    /// 
    /// # 引数
    /// * `repo_path` - リポジトリパス
    /// * `config` - 設定
    /// 
    /// # 戻り値
    /// * `Result<Vec<PathBuf>>` - エントリーポイントのリスト、またはエラー
    fn infer_entrypoints(&self, repo_path: &Path, config: &Config) -> Result<Vec<PathBuf>> {
        let mut entrypoints = Vec::new();

        // 設定で指定されたエントリーポイント
        for ep in &config.analysis.infer_entrypoints {
            let path = repo_path.join(ep);
            if path.exists() {
                entrypoints.push(path);
            }
        }

        // 一般的なエントリーポイントパターンを検索
        let patterns = vec![
            "main.ts", "main.js", "index.ts", "index.js",
            "main.py", "__main__.py",
            "main.go",
            "main.rs",
            "cmd/**/main.go",
            "apps/**/src/main.ts",
            "apps/**/src/index.ts",
        ];

        for pattern in patterns {
            if pattern.contains("**") {
                // ワイルドカードパターン
                for entry in WalkDir::new(repo_path) {
                    let entry = entry?;
                    if entry.file_type().is_file() {
                        let path = entry.path();
                        if path.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n == "main.ts" || n == "index.ts")
                            .unwrap_or(false)
                        {
                            entrypoints.push(path.to_path_buf());
                        }
                    }
                }
            } else {
                let path = repo_path.join(pattern);
                if path.exists() {
                    entrypoints.push(path);
                }
            }
        }

        Ok(entrypoints)
    }

    /// ファイルを除外すべきかチェック
    /// 
    /// # 引数
    /// * `path` - ファイルパス
    /// * `exclude_patterns` - 除外パターンのリスト
    /// 
    /// # 戻り値
    /// * `bool` - 除外すべき場合true
    fn should_exclude(&self, path: &Path, exclude_patterns: &[String]) -> bool {
        let path_str = path.to_string_lossy();
        for pattern in exclude_patterns {
            // 簡易的なglobマッチング（**と*をサポート）
            let regex_pattern = pattern
                .replace("**", ".*")
                .replace("*", "[^/]*")
                .replace(".", "\\.");
            if let Ok(re) = Regex::new(&format!("^{}$", regex_pattern)) {
                if re.is_match(&path_str) {
                    return true;
                }
            }
        }
        false
    }
}

/// インデックス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub id: String,
    pub repo_path: PathBuf,
    pub files: Vec<FileInfo>,
    pub modules: Vec<ModuleInfo>,
    pub languages: Vec<String>,
    pub dependencies: HashMap<String, Vec<String>>,
    pub entrypoints: Vec<PathBuf>,
    pub stats: IndexStats,
}

/// ファイル情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub language: String,
    pub size: usize,
    pub dependencies: Vec<String>,
    pub is_module: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// モジュール情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub path: PathBuf,
    pub name: String,
    pub language: String,
    pub dependencies: Vec<String>,
}

/// インデックス統計情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub files: usize,
    pub languages: Vec<String>,
    pub modules: usize,
}

impl Index {
    /// 検索を実行
    /// 
    /// # 引数
    /// * `query` - 検索クエリ
    /// * `k` - 返す結果の最大数
    /// 
    /// # 戻り値
    /// * `Result<Vec<SearchHit>>` - 検索結果、またはエラー
    pub async fn search(&self, query: &str, k: usize) -> Result<Vec<SearchHit>> {
        let mut hits = Vec::new();
        let query_lower = query.to_lowercase();

        for file in &self.files {
            if let Some(content) = &file.content {
                let content_lower = content.to_lowercase();
                if content_lower.contains(&query_lower) {
                    // 簡易的なマッチング（後でtantivyに置き換え可能）
                    let score = self.calculate_score(&content_lower, &query_lower);
                    let excerpt = self.extract_excerpt(content, &query_lower, 100);

                    hits.push(SearchHit {
                        path: file.path.to_string_lossy().to_string(),
                        score,
                        excerpt,
                    });
                }
            }
        }

        // スコアでソート
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(k);

        Ok(hits)
    }

    /// スコアを計算
    fn calculate_score(&self, content: &str, query: &str) -> f64 {
        let query_words: Vec<&str> = query.split_whitespace().collect();
        let mut score = 0.0;

        for word in &query_words {
            let count = content.matches(word).count();
            score += count as f64;
        }

        score / (query_words.len() as f64 + 1.0)
    }

    /// 抜粋を抽出
    fn extract_excerpt(&self, content: &str, query: &str, max_len: usize) -> String {
        if let Some(pos) = content.to_lowercase().find(query) {
            let start = pos.saturating_sub(max_len / 2);
            let end = (pos + query.len() + max_len / 2).min(content.len());
            let excerpt = &content[start..end];
            format!("...{}...", excerpt)
        } else {
            let excerpt = &content[..content.len().min(max_len)];
            format!("{}...", excerpt)
        }
    }
}

/// 検索ヒット
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchHit {
    pub path: String,
    pub score: f64,
    pub excerpt: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_js_dependencies() {
        let analyzer = Analyzer::new(Config::default());
        let content = r#"
import { foo } from './foo';
import bar from 'bar';
const baz = require('baz');
"#;
        let deps = analyzer.extract_js_dependencies(content);
        assert!(deps.contains(&"./foo".to_string()));
        assert!(deps.contains(&"bar".to_string()));
        assert!(deps.contains(&"baz".to_string()));
    }

    #[test]
    fn test_extract_py_dependencies() {
        let analyzer = Analyzer::new(Config::default());
        let content = r#"
import os
from pathlib import Path
import json
"#;
        let deps = analyzer.extract_py_dependencies(content);
        assert!(deps.contains(&"os".to_string()));
        assert!(deps.contains(&"pathlib".to_string()));
    }
}

