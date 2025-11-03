/**
 * GitHub Pages公開実装
 * 
 * 生成されたWikiやスライドをGitHub Pagesに公開する
 * - docs/モード: /docsディレクトリにコピー
 * - gh-pagesモード: gh-pagesブランチにコミット・プッシュ
 * - GitHub Actions YAMLの自動生成
 * 
 * 主な仕様:
 * - docs/モードはローカルでファイルをコピー
 * - gh-pagesモードはgit操作でブランチを更新
 * - Actions YAMLは任意で生成
 * 
 * 制限事項:
 * - gh-pagesモードはgit操作が必要（認証情報が必要な場合あり）
 * - Actions YAMLはテンプレートベース
 */

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;
use anyhow::{Context, Result};
use tracing::info;

use config::Config;
use git2::Repository;

/// パブリッシャー
pub struct Publisher {
    #[allow(dead_code)]
    config: Config,
}

impl Publisher {
    /// 新しいパブリッシャーインスタンスを作成
    /// 
    /// # 引数
    /// * `config` - 設定
    /// 
    /// # 戻り値
    /// * `Self` - パブリッシャーインスタンス
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// GitHub Pagesに公開
    /// 
    /// # 引数
    /// * `mode` - モード（docs|gh-pages）
    /// * `site_dir` - サイトディレクトリ
    /// * `slides_dir` - スライドディレクトリ
    /// * `repo_root` - リポジトリルート
    /// * `branch` - ブランチ名（gh-pagesモードの場合）
    /// 
    /// # 戻り値
    /// * `Result<PublishResult>` - 公開結果、またはエラー
    pub async fn publish(
        &self,
        mode: &str,
        site_dir: &str,
        slides_dir: &str,
        repo_root: &str,
        branch: &str,
    ) -> Result<PublishResult> {
        info!("GitHub Pages公開開始: mode={}", mode);

        match mode {
            "docs" => self.publish_docs(site_dir, slides_dir, repo_root).await,
            "gh-pages" => self.publish_gh_pages(site_dir, slides_dir, repo_root, branch).await,
            _ => Err(anyhow::anyhow!("不明なモード: {}", mode)),
        }
    }

    /// docs/モードで公開
    /// 
    /// # 引数
    /// * `site_dir` - サイトディレクトリ
    /// * `slides_dir` - スライドディレクトリ
    /// * `repo_root` - リポジトリルート
    /// 
    /// # 戻り値
    /// * `Result<PublishResult>` - 公開結果、またはエラー
    async fn publish_docs(
        &self,
        site_dir: &str,
        slides_dir: &str,
        repo_root: &str,
    ) -> Result<PublishResult> {
        info!("docs/モードで公開中...");

        let repo_path = PathBuf::from(repo_root);
        let docs_dir = repo_path.join("docs");

        // docs/ディレクトリを作成
        fs::create_dir_all(&docs_dir)?;

        // サイトをコピー
        let site_source = PathBuf::from(site_dir);
        if site_source.exists() {
            self.copy_directory(&site_source, &docs_dir)?;
            info!("サイトをdocs/にコピーしました");
        }

        // スライドをコピー（オプション）
        let slides_source = PathBuf::from(slides_dir);
        if slides_source.exists() {
            let slides_dest = docs_dir.join("slides");
            fs::create_dir_all(&slides_dest)?;
            self.copy_directory(&slides_source, &slides_dest)?;
            info!("スライドをdocs/slides/にコピーしました");
        }

        Ok(PublishResult {
            ok: true,
            hint: "リポジトリの設定で、GitHub Pagesのソースを 'main /docs' に設定してください。".to_string(),
        })
    }

    /// gh-pagesブランチモードで公開
    /// 
    /// # 引数
    /// * `site_dir` - サイトディレクトリ
    /// * `slides_dir` - スライドディレクトリ
    /// * `repo_root` - リポジトリルート
    /// * `branch` - ブランチ名
    /// 
    /// # 戻り値
    /// * `Result<PublishResult>` - 公開結果、またはエラー
    async fn publish_gh_pages(
        &self,
        site_dir: &str,
        slides_dir: &str,
        repo_root: &str,
        branch: &str,
    ) -> Result<PublishResult> {
        info!("gh-pagesブランチモードで公開中...");

        let repo = Repository::open(repo_root)
            .with_context(|| format!("リポジトリを開けませんでした: {}", repo_root))?;

        // 作業ディレクトリを一時的に作成
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path();

        // サイトをコピー
        let site_source = PathBuf::from(site_dir);
        if site_source.exists() {
            self.copy_directory(&site_source, temp_path)?;
        }

        // スライドをコピー
        let slides_source = PathBuf::from(slides_dir);
        if slides_source.exists() {
            let slides_dest = temp_path.join("slides");
            fs::create_dir_all(&slides_dest)?;
            self.copy_directory(&slides_source, &slides_dest)?;
        }

        // gh-pagesブランチにコミット
        self.commit_to_branch(&repo, branch, temp_path).await?;

        Ok(PublishResult {
            ok: true,
            hint: format!("gh-pagesブランチに公開しました。GitHub Pagesの設定でブランチ '{}' を選択してください。", branch),
        })
    }

    /// ディレクトリをコピー
    /// 
    /// # 引数
    /// * `source` - ソースディレクトリ
    /// * `dest` - 宛先ディレクトリ
    /// 
    /// # 戻り値
    /// * `Result<()>` - 成功、またはエラー
    fn copy_directory(&self, source: &Path, dest: &Path) -> Result<()> {
        if !source.exists() {
            return Err(anyhow::anyhow!("ソースディレクトリが存在しません: {:?}", source));
        }

        if source.is_file() {
            // ファイルの場合はコピー
            fs::copy(source, dest)?;
            return Ok(());
        }

        // ディレクトリの場合は再帰的にコピー
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let src_path = entry.path();
            let dest_path = dest.join(entry.file_name());

            if src_path.is_dir() {
                fs::create_dir_all(&dest_path)?;
                self.copy_directory(&src_path, &dest_path)?;
            } else {
                fs::copy(&src_path, &dest_path)?;
            }
        }

        Ok(())
    }

    /// ブランチにコミット
    /// 
    /// # 引数
    /// * `repo` - リポジトリ
    /// * `branch` - ブランチ名
    /// * `content_dir` - コンテンツディレクトリ
    /// 
    /// # 戻り値
    /// * `Result<()>` - 成功、またはエラー
    async fn commit_to_branch(
        &self,
        repo: &Repository,
        branch: &str,
        content_dir: &Path,
    ) -> Result<()> {
        // 簡易実装: gitコマンドを使用してブランチにコミット
        // 実際のプロダクション実装では、git2のAPIを使用して適切に実装する必要がある
        
        let repo_path = repo.path().parent().unwrap();
        
        // ブランチをチェックアウトまたは作成
        let output = Command::new("git")
            .arg("checkout")
            .arg("-b")
            .arg(branch)
            .current_dir(repo_path)
            .output();

        // ブランチが既に存在する場合はチェックアウト
        if let Err(_) = output {
            let output = Command::new("git")
                .arg("checkout")
                .arg(branch)
                .current_dir(repo_path)
                .output()?;
            if !output.status.success() {
                return Err(anyhow::anyhow!("ブランチのチェックアウトに失敗しました"));
            }
        }

        // すべてのファイルを削除（クリーンな状態にする）
        Command::new("git")
            .arg("rm")
            .arg("-rf")
            .arg(".")
            .current_dir(repo_path)
            .output()?;

        // コンテンツをコピー
        self.copy_directory(content_dir, repo_path)?;

        // ファイルを追加
        Command::new("git")
            .arg("add")
            .arg(".")
            .current_dir(repo_path)
            .output()?;

        // コミット
        Command::new("git")
            .arg("commit")
            .arg("-m")
            .arg("Update GitHub Pages")
            .current_dir(repo_path)
            .output()?;

        info!("{}ブランチにコミットしました", branch);

        Ok(())
    }

    /// GitHub Actions YAMLを生成
    /// 
    /// # 引数
    /// * `repo_root` - リポジトリルート
    /// 
    /// # 戻り値
    /// * `Result<PathBuf>` - 生成されたYAMLファイルのパス、またはエラー
    pub fn generate_actions_yaml(&self, repo_root: &str) -> Result<PathBuf> {
        info!("GitHub Actions YAMLを生成中...");

        let repo_path = PathBuf::from(repo_root);
        let workflows_dir = repo_path.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir)?;

        let yaml_content = r#"name: Deploy Pages

on:
  push:
    branches: ["main"]

permissions:
  contents: write

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --release
      - run: ./target/release/deeprepo-slides-mcp cli build-all
      - uses: peaceiris/actions-gh-pages@v4
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_branch: gh-pages
          publish_dir: out/wiki/book
"#;

        let yaml_path = workflows_dir.join("pages.yml");
        fs::write(&yaml_path, yaml_content)
            .with_context(|| format!("YAMLファイルの書き込みに失敗しました: {:?}", yaml_path))?;

        info!("GitHub Actions YAMLを生成しました: {:?}", yaml_path);
        Ok(yaml_path)
    }
}

/// 公開結果
#[derive(Debug, Serialize, Deserialize)]
pub struct PublishResult {
    pub ok: bool,
    pub hint: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copy_directory() {
        let config = Config::default();
        let publisher = Publisher::new(config);

        let temp_source = tempfile::tempdir().unwrap();
        let temp_dest = tempfile::tempdir().unwrap();

        let test_file = temp_source.path().join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        publisher.copy_directory(temp_source.path(), temp_dest.path()).unwrap();

        let copied_file = temp_dest.path().join("test.txt");
        assert!(copied_file.exists());
    }
}

