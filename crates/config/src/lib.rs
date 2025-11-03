/**
 * 設定ファイルパース用クレート
 * 
 * deeprepo.tomlの読み込みと設定値の管理を行う
 * 
 * 主な仕様:
 * - TOML形式の設定ファイルをパース
 * - デフォルト値の適用
 * - 設定値の検証
 * 
 * 制限事項:
 * - 環境変数の展開は行わない（呼び出し元で実装）
 */

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use thiserror::Error;

/// 設定ファイル全体の構造
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    #[serde(default)]
    pub project: ProjectConfig,
    #[serde(default)]
    pub analysis: AnalysisConfig,
    #[serde(default)]
    pub summarization: SummarizationConfig,
    #[serde(default)]
    pub index: IndexConfig,
    #[serde(default)]
    pub site: SiteConfig,
    #[serde(default)]
    pub slides: SlidesConfig,
    #[serde(default)]
    pub publish: PublishConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

/// プロジェクト設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectConfig {
    pub name: String,
    pub repo_path: PathBuf,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: "Unnamed Project".to_string(),
            repo_path: PathBuf::from("."),
            include: vec!["**/*".to_string()],
            exclude: vec![
                "**/node_modules/**".to_string(),
                "**/dist/**".to_string(),
                "**/.git/**".to_string(),
            ],
        }
    }
}

/// 解析設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AnalysisConfig {
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default = "default_max_file_kb")]
    pub max_file_kb: usize,
    #[serde(default)]
    pub infer_entrypoints: Vec<String>,
    #[serde(default)]
    pub diagrams: DiagramsConfig,
}

fn default_max_file_kb() -> usize {
    512
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            languages: vec!["ts".to_string(), "js".to_string()],
            max_file_kb: 512,
            infer_entrypoints: vec![],
            diagrams: DiagramsConfig::default(),
        }
    }
}

/// 図表設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DiagramsConfig {
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default = "default_diagram_renderer")]
    pub renderer: String,
}

fn default_diagram_renderer() -> String {
    "mermaid".to_string()
}

impl Default for DiagramsConfig {
    fn default() -> Self {
        Self {
            types: vec![
                "module-graph".to_string(),
                "call-graph".to_string(),
                "sequence".to_string(),
                "deployment".to_string(),
            ],
            renderer: "mermaid".to_string(),
        }
    }
}

/// 要約設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SummarizationConfig {
    #[serde(default = "default_summarization_mode")]
    pub mode: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_style")]
    pub style: String,
}

fn default_summarization_mode() -> String {
    "auto".to_string()
}

fn default_temperature() -> f64 {
    0.2
}

fn default_style() -> String {
    "concise-ja".to_string()
}

impl Default for SummarizationConfig {
    fn default() -> Self {
        Self {
            mode: "auto".to_string(),
            model: None,
            temperature: 0.2,
            style: "concise-ja".to_string(),
        }
    }
}

/// インデックス設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IndexConfig {
    #[serde(default = "default_index_provider")]
    pub provider: String,
    #[serde(default = "default_chunk_tokens")]
    pub chunk_tokens: usize,
    #[serde(default = "default_chunk_overlap")]
    pub chunk_overlap: usize,
}

fn default_index_provider() -> String {
    "tantivy".to_string()
}

fn default_chunk_tokens() -> usize {
    800
}

fn default_chunk_overlap() -> usize {
    120
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            provider: "tantivy".to_string(),
            chunk_tokens: 800,
            chunk_overlap: 120,
        }
    }
}

/// サイト設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SiteConfig {
    #[serde(default = "default_site_flavor")]
    pub flavor: String,
    #[serde(default = "default_site_out_dir")]
    pub out_dir: PathBuf,
}

fn default_site_flavor() -> String {
    "mdbook".to_string()
}

fn default_site_out_dir() -> PathBuf {
    PathBuf::from("./out/wiki")
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            flavor: "mdbook".to_string(),
            out_dir: PathBuf::from("./out/wiki"),
        }
    }
}

/// スライド設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SlidesConfig {
    #[serde(default = "default_slides_flavor")]
    pub flavor: String,
    #[serde(default = "default_slides_out_dir")]
    pub out_dir: PathBuf,
}

fn default_slides_flavor() -> String {
    "mdbook-reveal".to_string()
}

fn default_slides_out_dir() -> PathBuf {
    PathBuf::from("./out/slides")
}

impl Default for SlidesConfig {
    fn default() -> Self {
        Self {
            flavor: "mdbook-reveal".to_string(),
            out_dir: PathBuf::from("./out/slides"),
        }
    }
}

/// 公開設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PublishConfig {
    #[serde(default = "default_publish_mode")]
    pub mode: String,
    #[serde(default = "default_publish_branch")]
    pub branch: String,
}

fn default_publish_mode() -> String {
    "docs".to_string()
}

fn default_publish_branch() -> String {
    "gh-pages".to_string()
}

impl Default for PublishConfig {
    fn default() -> Self {
        Self {
            mode: "docs".to_string(),
            branch: "gh-pages".to_string(),
        }
    }
}

/// セキュリティ設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SecurityConfig {
    #[serde(default = "default_offline")]
    pub offline: bool,
    #[serde(default = "default_pii_redaction")]
    pub pii_redaction: bool,
}

fn default_offline() -> bool {
    true
}

fn default_pii_redaction() -> bool {
    true
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            offline: true,
            pii_redaction: true,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            project: ProjectConfig::default(),
            analysis: AnalysisConfig::default(),
            summarization: SummarizationConfig::default(),
            index: IndexConfig::default(),
            site: SiteConfig::default(),
            slides: SlidesConfig::default(),
            publish: PublishConfig::default(),
            security: SecurityConfig::default(),
            env: std::collections::HashMap::new(),
        }
    }
}

/// 設定ファイル読み込みエラー
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("設定ファイルが見つかりません: {0}")]
    FileNotFound(String),
    #[error("設定ファイルのパースに失敗しました: {0}")]
    ParseError(String),
    #[error("設定値の検証に失敗しました: {0}")]
    ValidationError(String),
}

impl Config {
    /// 設定ファイルを読み込む
    /// 
    /// # 引数
    /// * `config_path` - 設定ファイルのパス（Noneの場合はデフォルト設定を返す）
    /// 
    /// # 戻り値
    /// * `Result<Config>` - 読み込んだ設定、またはエラー
    pub fn load<P: AsRef<Path>>(config_path: Option<P>) -> Result<Self> {
        let config_path = match config_path {
            Some(p) => p.as_ref().to_path_buf(),
            None => return Ok(Self::default()),
        };

        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("設定ファイルの読み込みに失敗しました: {:?}", config_path))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("設定ファイルのパースに失敗しました: {:?}", config_path))?;

        config.validate()?;
        Ok(config)
    }

    /// 設定値の検証を行う
    /// 
    /// # 戻り値
    /// * `Result<()>` - 検証成功、またはエラー
    pub fn validate(&self) -> Result<()> {
        if !self.project.repo_path.exists() {
            return Err(anyhow::anyhow!(
                "リポジトリパスが存在しません: {:?}",
                self.project.repo_path
            ));
        }

        if self.analysis.max_file_kb == 0 {
            return Err(anyhow::anyhow!("max_file_kbは0より大きい値である必要があります"));
        }

        if !["mermaid", "graphviz"].contains(&self.analysis.diagrams.renderer.as_str()) {
            return Err(anyhow::anyhow!(
                "diagrams.rendererは 'mermaid' または 'graphviz' である必要があります"
            ));
        }

        if !["none", "auto", "local", "remote"].contains(&self.summarization.mode.as_str()) {
            return Err(anyhow::anyhow!(
                "summarization.modeは 'none', 'auto', 'local', 'remote' のいずれかである必要があります"
            ));
        }

        if !["docs", "gh-pages"].contains(&self.publish.mode.as_str()) {
            return Err(anyhow::anyhow!(
                "publish.modeは 'docs' または 'gh-pages' である必要があります"
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.project.name, "Unnamed Project");
        assert_eq!(config.analysis.max_file_kb, 512);
    }

    #[test]
    fn test_config_load_none() {
        let config = Config::load::<PathBuf>(None).unwrap();
        assert_eq!(config.project.name, "Unnamed Project");
    }
}

