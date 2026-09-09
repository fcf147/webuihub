#!/usr/bin/env bash
# ============================================================================
# bootstrap-hermes.sh — 在 WSL 发行版内安装 Hermes Agent
# ----------------------------------------------------------------------------
# 由 WebUI Shell 在 WSL 内通过 /bin/bash 执行（脚本内容经 stdin 喂给 bash）。
# 输出逐行回传桌面壳日志面板。
#
# Hermes Agent 官方推荐安装方式：
#   curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash
# 该引导脚本会克隆 NousResearch/hermes-agent 仓库、创建 Python venv、
# 按需安装 Node/浏览器(Playwright)/Computer Use 驱动，并把 `hermes` 命令加入 PATH。
# 依赖前置由官方脚本自行处理，本脚本不再重复安装运行环境。
# ============================================================================
set -eu
# pipefail 仅 bash 支持；在 sh/dash 下跳过，避免 "set: pipefail: invalid option name"
if [ -n "${BASH_VERSION:-}" ]; then set -o pipefail; fi

echo "==> 安装 Hermes Agent（官方引导脚本）..."

# 官方安装器：下载并执行远程 install.sh。
# 注：脚本会把 ~/.local/bin（或 root 的 /usr/local/bin）加入 PATH 并写入 shell 配置。
curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash

echo "==> 验证安装..."
if command -v hermes >/dev/null 2>&1; then
  hermes --version 2>/dev/null || echo "（hermes 已安装，--version 暂未输出）"
else
  echo "未在当前 PATH 检测到 hermes 命令；官方脚本已将其加入 shell 配置，请重开终端或 source 对应 rc 文件。"
fi

echo "==> Hermes Agent 安装完成"
