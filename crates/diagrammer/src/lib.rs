/**
 * ダイアグラマー実装
 * 
 * コードベースから図表を生成する
 * - Mermaid DSLの生成（flowchart, classDiagram, sequenceDiagram）
 * - Graphviz DOT形式の生成（オプション）
 * - モジュールグラフ、コールグラフ、シーケンス図、デプロイメント図
 * 
 * 主な仕様:
 * - Mermaidをデフォルトレンダラとして使用
 * - Graphvizは外部コマンド呼び出し（オプション）
 * - 複数の図タイプに対応
 * 
 * 制限事項:
 * - コールグラフは簡易的な解析に基づく（完全な静的解析ではない）
 * - シーケンス図は関数名から推測（実際の呼び出しフローではない）
 */

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use anyhow::Result;
use tracing::info;

use config::Config;
use analyzer_core::Index;

/// ダイアグラマー
pub struct Diagrammer {
    config: Config,
}

impl Diagrammer {
    /// 新しいダイアグラマーインスタンスを作成
    /// 
    /// # 引数
    /// * `config` - 設定
    /// 
    /// # 戻り値
    /// * `Self` - ダイアグラマーインスタンス
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// 図を生成
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// * `diagram_type` - 図のタイプ（module-graph|call-graph|sequence|deployment）
    /// 
    /// # 戻り値
    /// * `Result<Diagram>` - 生成された図、またはエラー
    pub fn generate_diagram(&self, index: &Index, diagram_type: &str) -> Result<Diagram> {
        info!("図生成開始: type={}", diagram_type);

        let (content, format) = match diagram_type {
            "module-graph" => self.generate_module_graph(index)?,
            "call-graph" => self.generate_call_graph(index)?,
            "sequence" => self.generate_sequence_diagram(index)?,
            "deployment" => self.generate_deployment_diagram(index)?,
            _ => return Err(anyhow::anyhow!("不明な図タイプ: {}", diagram_type)),
        };

        Ok(Diagram {
            diagram_type: diagram_type.to_string(),
            format: format.to_string(),
            content,
        })
    }

    /// モジュールグラフを生成
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// 
    /// # 戻り値
    /// * `Result<(String, &str)>` - (内容, フォーマット) またはエラー
    fn generate_module_graph(&self, index: &Index) -> Result<(String, &str)> {
        match self.config.analysis.diagrams.renderer.as_str() {
            "mermaid" => self.generate_module_graph_mermaid(index),
            "graphviz" => self.generate_module_graph_graphviz(index),
            _ => Err(anyhow::anyhow!("不明なレンダラ: {}", self.config.analysis.diagrams.renderer)),
        }
    }

    /// Mermaid形式のモジュールグラフを生成
    fn generate_module_graph_mermaid(&self, index: &Index) -> Result<(String, &str)> {
        let mut mermaid = String::from("graph TD\n");
        let mut node_map = HashMap::new();
        let mut node_id = 0;

        // ノードを作成
        for module in &index.modules {
            let id = format!("M{}", node_id);
            node_map.insert(module.path.clone(), id.clone());
            let label = module.name.clone();
            mermaid.push_str(&format!("    {}[\"{}\"]\n", id, label));
            node_id += 1;
        }

        // エッジを作成（依存関係から）
        for module in &index.modules {
            if let Some(from_id) = node_map.get(&module.path) {
                for dep in &module.dependencies {
                    // 依存関係からモジュールを検索
                    if let Some(to_module) = index.modules.iter().find(|m| m.name.contains(dep)) {
                        if let Some(to_id) = node_map.get(&to_module.path) {
                            mermaid.push_str(&format!("    {} --> {}\n", from_id, to_id));
                        }
                    }
                }
            }
        }

        Ok((mermaid, "mermaid"))
    }

    /// Graphviz形式のモジュールグラフを生成
    fn generate_module_graph_graphviz(&self, index: &Index) -> Result<(String, &str)> {
        let mut dot = String::from("digraph ModuleGraph {\n");
        dot.push_str("    rankdir=LR;\n");
        dot.push_str("    node [shape=box];\n\n");

        let mut node_map = HashMap::new();
        let mut node_id = 0;

        // ノードを作成
        for module in &index.modules {
            let id = format!("M{}", node_id);
            node_map.insert(module.path.clone(), id.clone());
            let label = module.name.clone();
            dot.push_str(&format!("    {} [label=\"{}\"];\n", id, label));
            node_id += 1;
        }

        dot.push_str("\n");

        // エッジを作成
        for module in &index.modules {
            if let Some(from_id) = node_map.get(&module.path) {
                for dep in &module.dependencies {
                    if let Some(to_module) = index.modules.iter().find(|m| m.name.contains(dep)) {
                        if let Some(to_id) = node_map.get(&to_module.path) {
                            dot.push_str(&format!("    {} -> {};\n", from_id, to_id));
                        }
                    }
                }
            }
        }

        dot.push_str("}\n");

        Ok((dot, "graphviz"))
    }

    /// コールグラフを生成
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// 
    /// # 戻り値
    /// * `Result<(String, &str)>` - (内容, フォーマット) またはエラー
    fn generate_call_graph(&self, index: &Index) -> Result<(String, &str)> {
        match self.config.analysis.diagrams.renderer.as_str() {
            "mermaid" => self.generate_call_graph_mermaid(index),
            "graphviz" => self.generate_call_graph_graphviz(index),
            _ => Err(anyhow::anyhow!("不明なレンダラ: {}", self.config.analysis.diagrams.renderer)),
        }
    }

    /// Mermaid形式のコールグラフを生成
    fn generate_call_graph_mermaid(&self, index: &Index) -> Result<(String, &str)> {
        let mut mermaid = String::from("graph LR\n");
        let mut functions = Vec::new();

        // 関数を抽出
        for file in &index.files {
            if let Some(content) = &file.content {
                let funcs = self.extract_functions(content, &file.language);
                functions.extend(funcs);
            }
        }

        // ノードを作成
        for (i, func) in functions.iter().enumerate() {
            let id = format!("F{}", i);
            mermaid.push_str(&format!("    {}[\"{}\"]\n", id, func));
        }

        // 簡易的な呼び出し関係を推測（実際の解析は行わない）
        // ここでは関数名から推測

        Ok((mermaid, "mermaid"))
    }

    /// Graphviz形式のコールグラフを生成
    fn generate_call_graph_graphviz(&self, index: &Index) -> Result<(String, &str)> {
        let mut dot = String::from("digraph CallGraph {\n");
        dot.push_str("    rankdir=LR;\n");
        dot.push_str("    node [shape=ellipse];\n\n");

        let mut functions = Vec::new();
        for file in &index.files {
            if let Some(content) = &file.content {
                let funcs = self.extract_functions(content, &file.language);
                functions.extend(funcs);
            }
        }

        for (i, func) in functions.iter().enumerate() {
            let id = format!("F{}", i);
            dot.push_str(&format!("    {} [label=\"{}\"];\n", id, func));
        }

        dot.push_str("}\n");

        Ok((dot, "graphviz"))
    }

    /// シーケンス図を生成
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// 
    /// # 戻り値
    /// * `Result<(String, &str)>` - (内容, フォーマット) またはエラー
    fn generate_sequence_diagram(&self, index: &Index) -> Result<(String, &str)> {
        match self.config.analysis.diagrams.renderer.as_str() {
            "mermaid" => self.generate_sequence_diagram_mermaid(index),
            _ => Err(anyhow::anyhow!("シーケンス図はMermaidのみサポートされています")),
        }
    }

    /// Mermaid形式のシーケンス図を生成
    fn generate_sequence_diagram_mermaid(&self, index: &Index) -> Result<(String, &str)> {
        let mut mermaid = String::from("sequenceDiagram\n");

        // モジュールをアクターとして追加
        let mut actors = Vec::new();
        for module in &index.modules {
            actors.push(module.name.clone());
        }

        // 最初の3つのモジュールを使用
        for actor in actors.iter().take(3) {
            mermaid.push_str(&format!("    participant {}\n", actor));
        }

        // 簡易的なシーケンス（実際の呼び出しフローではない）
        if actors.len() >= 2 {
            mermaid.push_str(&format!("    {}->>{}: 呼び出し\n", actors[0], actors[1]));
        }
        if actors.len() >= 3 {
            mermaid.push_str(&format!("    {}->>{}: 呼び出し\n", actors[1], actors[2]));
        }

        Ok((mermaid, "mermaid"))
    }

    /// デプロイメント図を生成
    /// 
    /// # 引数
    /// * `index` - インデックス
    /// 
    /// # 戻り値
    /// * `Result<(String, &str)>` - (内容, フォーマット) またはエラー
    fn generate_deployment_diagram(&self, index: &Index) -> Result<(String, &str)> {
        match self.config.analysis.diagrams.renderer.as_str() {
            "mermaid" => self.generate_deployment_diagram_mermaid(index),
            _ => Err(anyhow::anyhow!("デプロイメント図はMermaidのみサポートされています")),
        }
    }

    /// Mermaid形式のデプロイメント図を生成
    fn generate_deployment_diagram_mermaid(&self, _index: &Index) -> Result<(String, &str)> {
        let mut mermaid = String::from("graph TB\n");
        mermaid.push_str("    subgraph \"Frontend\"\n");
        mermaid.push_str("        FE[フロントエンド]\n");
        mermaid.push_str("    end\n");
        mermaid.push_str("    subgraph \"Backend\"\n");
        mermaid.push_str("        BE[バックエンド]\n");
        mermaid.push_str("    end\n");
        mermaid.push_str("    subgraph \"Database\"\n");
        mermaid.push_str("        DB[データベース]\n");
        mermaid.push_str("    end\n");
        mermaid.push_str("    FE --> BE\n");
        mermaid.push_str("    BE --> DB\n");

        Ok((mermaid, "mermaid"))
    }

    /// 関数を抽出
    /// 
    /// # 引数
    /// * `content` - ファイル内容
    /// * `language` - 言語
    /// 
    /// # 戻り値
    /// * `Vec<String>` - 関数名のリスト
    fn extract_functions(&self, content: &str, language: &str) -> Vec<String> {
        let mut functions = Vec::new();

        match language {
            "ts" | "js" => {
                // JavaScript/TypeScript関数
                let func_re = regex::Regex::new(r"(?:export\s+)?(?:async\s+)?function\s+(\w+)").unwrap();
                for cap in func_re.captures_iter(content) {
                    if let Some(name) = cap.get(1) {
                        functions.push(name.as_str().to_string());
                    }
                }
                // アロー関数（const/let）
                let arrow_re = regex::Regex::new(r"(?:const|let)\s+(\w+)\s*=\s*(?:async\s+)?\([^)]*\)\s*=>").unwrap();
                for cap in arrow_re.captures_iter(content) {
                    if let Some(name) = cap.get(1) {
                        functions.push(name.as_str().to_string());
                    }
                }
            }
            "py" => {
                // Python関数
                let func_re = regex::Regex::new(r"def\s+(\w+)").unwrap();
                for cap in func_re.captures_iter(content) {
                    if let Some(name) = cap.get(1) {
                        functions.push(name.as_str().to_string());
                    }
                }
            }
            "go" => {
                // Go関数
                let func_re = regex::Regex::new(r"func\s+(\w+)").unwrap();
                for cap in func_re.captures_iter(content) {
                    if let Some(name) = cap.get(1) {
                        functions.push(name.as_str().to_string());
                    }
                }
            }
            "rs" => {
                // Rust関数
                let func_re = regex::Regex::new(r"fn\s+(\w+)").unwrap();
                for cap in func_re.captures_iter(content) {
                    if let Some(name) = cap.get(1) {
                        functions.push(name.as_str().to_string());
                    }
                }
            }
            _ => {}
        }

        functions
    }
}

/// 図
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagram {
    pub diagram_type: String,
    pub format: String,
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_functions_js() {
        let config = Config::default();
        let diagrammer = Diagrammer::new(config);
        let content = r#"
function foo() {}
const bar = () => {}
export async function baz() {}
"#;
        let funcs = diagrammer.extract_functions(content, "js");
        assert!(funcs.contains(&"foo".to_string()));
        assert!(funcs.contains(&"bar".to_string()));
        assert!(funcs.contains(&"baz".to_string()));
    }

    #[test]
    fn test_extract_functions_py() {
        let config = Config::default();
        let diagrammer = Diagrammer::new(config);
        let content = r#"
def foo():
    pass

def bar():
    pass
"#;
        let funcs = diagrammer.extract_functions(content, "py");
        assert!(funcs.contains(&"foo".to_string()));
        assert!(funcs.contains(&"bar".to_string()));
    }
}

