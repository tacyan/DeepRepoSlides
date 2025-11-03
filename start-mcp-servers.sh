#!/bin/bash
# MCPサーバーを16並列で起動するスクリプト
# 
# 各tmuxペインでdeeprepo-slides-mcpをMCPサーバーとして起動します
# 
# 主な仕様:
# - swarm-multiagentセッションの0番ウィンドウの0-15番ペインで実行
# - 各ペインでdeeprepo-slides-mcpをMCPサーバーとして起動
# 
# 制限事項:
# - tmuxセッションが存在する必要があります
# - 各ペインで実行中のプロセスは上書きされます

# 作業ディレクトリに移動
cd /Users/tacyan/dev/DeepRepoSlides

# 環境変数を設定
export RUN_AS_MCP=1

# 16個のペインでMCPサーバーを起動
for i in {0..15}; do
    tmux send-keys -t swarm-multiagent:0.$i "cd /Users/tacyan/dev/DeepRepoSlides" C-m
    tmux send-keys -t swarm-multiagent:0.$i "export RUN_AS_MCP=1" C-m
    tmux send-keys -t swarm-multiagent:0.$i "./target/release/deeprepo-slides-mcp" C-m
    sleep 0.1
done

echo "16個のペインでdeeprepo-slides-mcpを起動しました"
echo "各ペインでMCPサーバーが実行中です"

