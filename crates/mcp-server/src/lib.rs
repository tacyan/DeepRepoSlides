/**
 * MCPサーバー実装
 * 
 * JSON-RPC over stdioでMCPクライアントと通信し、
 * リポジトリ解析、要約生成、Wiki/スライド生成、公開などの機能を提供する
 * 
 * 主な仕様:
 * - JSON-RPC 2.0プロトコルに準拠
 * - 標準入出力経由で通信
 * - ツール: index_repo, summarize, generate_wiki, generate_slides, publish_pages, search
 * 
 * 制限事項:
 * - リクエストの並列処理は現在サポートしていない（順次処理）
 */

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tracing::{debug, error, info};
use chrono::Utc;

use config::Config;
use analyzer_core::{Analyzer, Index, IndexStats, SearchHit};
use summarizer::{Summarizer, SummarizeResult};
use diagrammer::Diagrammer;
use site_mdbook::{MdBookBuilder, WikiResult};
use slides::{SlideBuilder, SlideResult};
use publisher_ghpages::{Publisher, PublishResult};

/// MCPサーバーの実装
pub struct McpServer {
    /// 設定
    config: Config,
    /// インデックスストレージ（index_id -> Index）
    indexes: Arc<RwLock<HashMap<String, Index>>>,
    /// アナライザー
    analyzer: Arc<Analyzer>,
    /// サマライザー
    summarizer: Arc<Summarizer>,
    /// ダイアグラマー
    #[allow(dead_code)]
    diagrammer: Arc<Diagrammer>,
}

impl McpServer {
    /// 新しいMCPサーバーインスタンスを作成
    /// 
    /// # 引数
    /// * `config` - 設定
    /// 
    /// # 戻り値
    /// * `Self` - MCPサーバーインスタンス
    pub fn new(config: Config) -> Self {
        Self {
            config: config.clone(),
            indexes: Arc::new(RwLock::new(HashMap::new())),
            analyzer: Arc::new(Analyzer::new(config.clone())),
            summarizer: Arc::new(Summarizer::new(config.clone())),
            diagrammer: Arc::new(Diagrammer::new(config.clone())),
        }
    }

    /// MCPサーバーを起動し、標準入出力でリクエストを処理
    /// 
    /// # 戻り値
    /// * `anyhow::Result<()>` - 処理成功、またはエラー
    pub async fn serve(&self) -> anyhow::Result<()> {
        info!("DeepRepoSlides MCPサーバーを起動しました");

        let stdin = io::stdin();
        let mut stdin_reader = BufReader::new(stdin).lines();
        let mut stdout = io::stdout();

        loop {
            tokio::select! {
                result = stdin_reader.next_line() => {
                    match result {
                        Ok(Some(line)) => {
                            if let Err(e) = self.handle_request(&line, &mut stdout).await {
                                error!("リクエスト処理エラー: {}", e);
                                let error_response = self.create_error_response(
                                    None,
                                    -32603,
                                    &format!("内部エラー: {}", e),
                                );
                                self.write_response(&mut stdout, &error_response).await?;
                            }
                        }
                        Ok(None) => {
                            debug!("標準入力が閉じられました");
                            break;
                        }
                        Err(e) => {
                            error!("標準入力読み込みエラー: {}", e);
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// JSON-RPCリクエストを処理
    /// 
    /// # 引数
    /// * `line` - JSON-RPCリクエスト文字列
    /// * `stdout` - 標準出力ライター
    /// 
    /// # 戻り値
    /// * `anyhow::Result<()>` - 処理成功、またはエラー
    async fn handle_request(
        &self,
        line: &str,
        stdout: &mut io::Stdout,
    ) -> anyhow::Result<()> {
        debug!("リクエスト受信: {}", line);

        let request: JsonRpcRequest = match serde_json::from_str(line) {
            Ok(req) => req,
            Err(e) => {
                let error_response = self.create_error_response(
                    None,
                    -32700,
                    &format!("パースエラー: {}", e),
                );
                self.write_response(stdout, &error_response).await?;
                return Ok(());
            }
        };

        let response = match self.dispatch_tool(&request.method, request.params).await {
            Ok(result) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(serde_json::to_value(result)?),
                error: None,
            },
            Err(e) => {
                error!("ツール実行エラー: {}", e);
                self.create_error_response(
                    request.id,
                    -32603,
                    &format!("ツール実行エラー: {}", e),
                )
            }
        };

        self.write_response(stdout, &response).await?;
        Ok(())
    }

    /// ツールをディスパッチ
    /// 
    /// # 引数
    /// * `method` - ツール名
    /// * `params` - パラメータ
    /// 
    /// # 戻り値
    /// * `anyhow::Result<Value>` - 結果、またはエラー
    async fn dispatch_tool(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        match method {
            "index_repo" => {
                let args: IndexRepoArgs = serde_json::from_value(params)?;
                let result = self.index_repo(args).await?;
                Ok(serde_json::to_value(result)?)
            }
            "summarize" => {
                let args: SummarizeArgs = serde_json::from_value(params)?;
                let result = self.summarize(args).await?;
                Ok(serde_json::to_value(result)?)
            }
            "generate_wiki" => {
                let args: GenerateWikiArgs = serde_json::from_value(params)?;
                let result = self.generate_wiki(args).await?;
                Ok(serde_json::to_value(result)?)
            }
            "generate_slides" => {
                let args: GenerateSlidesArgs = serde_json::from_value(params)?;
                let result = self.generate_slides(args).await?;
                Ok(serde_json::to_value(result)?)
            }
            "publish_pages" => {
                let args: PublishPagesArgs = serde_json::from_value(params)?;
                let result = self.publish_pages(args).await?;
                Ok(serde_json::to_value(result)?)
            }
            "search" => {
                let args: SearchArgs = serde_json::from_value(params)?;
                let result = self.search(args).await?;
                Ok(serde_json::to_value(result)?)
            }
            _ => Err(anyhow::anyhow!("不明なツール: {}", method)),
        }
    }

    /// リポジトリをインデックス化
    /// 
    /// # 引数
    /// * `args` - インデックス化パラメータ
    /// 
    /// # 戻り値
    /// * `anyhow::Result<IndexRepoResult>` - 結果、またはエラー
    async fn index_repo(&self, args: IndexRepoArgs) -> anyhow::Result<IndexRepoResult> {
        info!("リポジトリをインデックス化中: {:?}", args.repo_path);

        let config = if let Some(config_path) = args.config {
            Config::load(Some(config_path))?
        } else {
            self.config.clone()
        };

        let index = self.analyzer.analyze_repo(&args.repo_path, &config).await?;
        let index_id = format!("idx_{}", Utc::now().format("%Y%m%d_%H%M%S"));

        {
            let mut indexes = self.indexes.write().await;
            indexes.insert(index_id.clone(), index.clone());
        }

        let stats = IndexStats {
            files: index.files.len(),
            languages: index.languages.clone(),
            modules: index.modules.len(),
        };

        Ok(IndexRepoResult {
            ok: true,
            index_id,
            stats,
        })
    }

    /// 要約を生成
    /// 
    /// # 引数
    /// * `args` - 要約パラメータ
    /// 
    /// # 戻り値
    /// * `anyhow::Result<SummarizeResult>` - 結果、またはエラー
    async fn summarize(&self, args: SummarizeArgs) -> anyhow::Result<SummarizeResult> {
        info!("要約生成中: scope={}, target={}", args.scope, args.target);

        let indexes = self.indexes.read().await;
        let index = indexes
            .values()
            .next()
            .ok_or_else(|| anyhow::anyhow!("インデックスが見つかりません"))?;

        let result = self
            .summarizer
            .summarize(index, &args.scope, &args.target, &args.style)
            .await?;

        Ok(result)
    }

    /// Wikiを生成
    /// 
    /// # 引数
    /// * `args` - Wiki生成パラメータ
    /// 
    /// # 戻り値
    /// * `anyhow::Result<WikiResult>` - 結果、またはエラー
    async fn generate_wiki(&self, args: GenerateWikiArgs) -> anyhow::Result<WikiResult> {
        info!("Wiki生成中: index_id={}", args.index_id);

        let indexes = self.indexes.read().await;
        let index = indexes
            .get(&args.index_id)
            .ok_or_else(|| anyhow::anyhow!("インデックスが見つかりません: {}", args.index_id))?;

        let builder = MdBookBuilder::new(self.config.clone());
        let result = builder
            .build_wiki(
                index,
                &args.out_dir.unwrap_or_else(|| "./out/wiki".into()),
                args.with_diagrams,
                &args.toc,
            )
            .await?;

        Ok(result)
    }

    /// スライドを生成
    /// 
    /// # 引数
    /// * `args` - スライド生成パラメータ
    /// 
    /// # 戻り値
    /// * `anyhow::Result<SlideResult>` - 結果、またはエラー
    async fn generate_slides(&self, args: GenerateSlidesArgs) -> anyhow::Result<SlideResult> {
        info!("スライド生成中: index_id={}", args.index_id);

        let indexes = self.indexes.read().await;
        let index = indexes
            .get(&args.index_id)
            .ok_or_else(|| anyhow::anyhow!("インデックスが見つかりません: {}", args.index_id))?;

        let builder = SlideBuilder::new(self.config.clone());
        let result = builder
            .build_slides(
                index,
                &args.flavor,
                &args.out_dir.unwrap_or_else(|| "./out/slides".into()),
                &args.sections,
                &args.export,
            )
            .await?;

        Ok(result)
    }

    /// GitHub Pagesに公開
    /// 
    /// # 引数
    /// * `args` - 公開パラメータ
    /// 
    /// # 戻り値
    /// * `anyhow::Result<PublishResult>` - 結果、またはエラー
    async fn publish_pages(&self, args: PublishPagesArgs) -> anyhow::Result<PublishResult> {
        info!("GitHub Pages公開中: mode={}", args.mode);

        let publisher = Publisher::new(self.config.clone());
        let result = publisher
            .publish(
                &args.mode,
                &args.site_dir,
                &args.slides_dir,
                &args.repo_root,
                &args.branch,
            )
            .await?;

        Ok(result)
    }

    /// 検索を実行
    /// 
    /// # 引数
    /// * `args` - 検索パラメータ
    /// 
    /// # 戻り値
    /// * `anyhow::Result<SearchResult>` - 結果、またはエラー
    async fn search(&self, args: SearchArgs) -> anyhow::Result<SearchResult> {
        info!("検索実行中: q={}", args.q);

        let indexes = self.indexes.read().await;
        let index = indexes
            .values()
            .next()
            .ok_or_else(|| anyhow::anyhow!("インデックスが見つかりません"))?;

        let hits = index.search(&args.q, args.k).await?;

        Ok(SearchResult { ok: true, hits })
    }

    /// エラーレスポンスを作成
    fn create_error_response(&self, id: Option<Value>, code: i32, message: &str) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.to_string(),
                data: None,
            }),
        }
    }

    /// レスポンスを書き込み
    async fn write_response(&self, stdout: &mut io::Stdout, response: &JsonRpcResponse) -> anyhow::Result<()> {
        let json = serde_json::to_string(response)?;
        stdout.write_all(json.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
        Ok(())
    }
}

/// JSON-RPCリクエスト
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    method: String,
    params: Value,
    id: Option<Value>,
}

/// JSON-RPCレスポンス
#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

/// JSON-RPCエラー
#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

/// index_repoツールの引数
#[derive(Debug, Deserialize)]
struct IndexRepoArgs {
    repo_path: String,
    #[serde(default)]
    config: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    refresh: bool,
}

/// index_repoツールの結果
#[derive(Debug, Serialize)]
struct IndexRepoResult {
    ok: bool,
    index_id: String,
    stats: IndexStats,
}

/// summarizeツールの引数
#[derive(Debug, Deserialize)]
struct SummarizeArgs {
    scope: String,
    target: String,
    #[serde(default = "default_style")]
    style: String,
}

fn default_style() -> String {
    "concise-ja".to_string()
}

/// generate_wikiツールの引数
#[derive(Debug, Deserialize)]
struct GenerateWikiArgs {
    index_id: String,
    #[serde(default)]
    out_dir: Option<String>,
    #[serde(default)]
    with_diagrams: bool,
    #[serde(default)]
    toc: Vec<String>,
}

/// generate_slidesツールの引数
#[derive(Debug, Deserialize)]
struct GenerateSlidesArgs {
    index_id: String,
    #[serde(default = "default_flavor")]
    flavor: String,
    #[serde(default)]
    out_dir: Option<String>,
    #[serde(default)]
    sections: Vec<String>,
    #[serde(default)]
    export: Vec<String>,
}

fn default_flavor() -> String {
    "mdbook-reveal".to_string()
}

/// publish_pagesツールの引数
#[derive(Debug, Deserialize)]
struct PublishPagesArgs {
    mode: String,
    site_dir: String,
    slides_dir: String,
    repo_root: String,
    #[serde(default = "default_branch")]
    branch: String,
}

fn default_branch() -> String {
    "gh-pages".to_string()
}

/// searchツールの引数
#[derive(Debug, Deserialize)]
struct SearchArgs {
    q: String,
    #[serde(default = "default_k")]
    k: usize,
}

fn default_k() -> usize {
    20
}

/// searchツールの結果
#[derive(Debug, Serialize)]
struct SearchResult {
    ok: bool,
    hits: Vec<SearchHit>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonrpc_request_deserialize() {
        let json = r#"{"jsonrpc":"2.0","method":"index_repo","params":{"repo_path":"."},"id":1}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "index_repo");
    }
}

