#!/usr/bin/env bash
# ============================================================================
# bootstrap-hermes.sh — 在 WSL 发行版内安装 Hermes Agent（含 Web Dashboard 依赖）
# ----------------------------------------------------------------------------
# 由 WebUI Shell 在 WSL 内通过 /bin/bash 执行（脚本内容经 stdin 喂给 bash）。
# 输出逐行回传桌面壳日志面板。
#
# 官方安装器：
#   curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash
#
# 坑点（已在本脚本规避）：
#   - 中国网络下官方安装器会重定向到 CN 镜像并进入 "minimal mode"，
#     跳过 Node.js 与 Web 前端、只装 core extra。而 hermes dashboard 的后端是
#     Python [web,pty] extra、前端是 React/Vite（首次启动需 Node 构建），
#     缺少这些会导致 dashboard 起不来、端口 9119 不开。
#     => 这里先主动安装 Node.js，再在安装后补装 [web,pty] extra。
# ============================================================================
set -eu
# pipefail 仅 bash 支持；在 sh/dash 下跳过，避免 "set: pipefail: invalid option name"
if [ -n "${BASH_VERSION:-}" ]; then set -o pipefail; fi

echo "==> 预装 Node.js（Hermes Dashboard 前端为 React/Vite，需 Node 构建；CN 镜像 minimal 模式会跳过，这里主动补齐）..."
install_node() {
  if command -v node >/dev/null 2>&1; then
    echo "    Node 已存在 ($(node -v))，跳过安装"
    return 0
  fi
  . /etc/os-release 2>/dev/null || true
  local DISTRO="${ID:-unknown}"
  echo "    发行版: ${DISTRO}"
  case "$DISTRO" in
    ubuntu|debian)
      curl -fsSL https://deb.nodesource.com/setup_24.x | bash -
      apt-get update && apt-get install -y nodejs
      ;;
    fedora)
      dnf install -y nodejs npm
      ;;
    centos|rhel|rocky|almalinux)
      curl -fsSL https://rpm.nodesource.com/setup_24.x | bash -
      yum install -y nodejs
      ;;
    arch|manjaro)
      pacman -Sy --noconfirm nodejs npm
      ;;
    opensuse*|suse)
      zypper --non-interactive install nodejs npm
      ;;
    alpine)
      apk add --no-cache nodejs npm
      ;;
    *)
      echo "未能识别发行版 ${DISTRO}，请手动安装 Node.js 22.22+/24.11+/26+ 后重试。" >&2
      return 1
      ;;
  esac
}
install_node

echo "==> 安装 Hermes Agent（官方引导脚本）..."
curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash

echo "==> 补全 Web Dashboard 后端依赖（[web,pty] extra）..."
# 安装目录：root 用 FHS 布局 /usr/local/lib/hermes-agent；普通用户用 ~/.hermes/hermes-agent
HERMES_DIR="/usr/local/lib/hermes-agent"
[ -d "$HERMES_DIR/venv" ] || HERMES_DIR="$HOME/.hermes/hermes-agent"
if [ -d "$HERMES_DIR/venv" ] && [ -f "$HERMES_DIR/venv/bin/activate" ]; then
  echo "    venv: $HERMES_DIR/venv"
  # 在 venv 内安装 dashboard 所需 Python 依赖；失败不中断，给出手动提示。
  # 优先用 uv（安装器可能把受管 uv 放在 /usr/local/share/uv 下但未加入 PATH），
  # 找不到时回退 venv 自带的 pip（python -m pip）。
  ( cd "$HERMES_DIR" && source venv/bin/activate \
      && if command -v uv >/dev/null 2>&1; then uv pip install -e ".[web,pty]"; else python -m pip install -e ".[web,pty]"; fi ) \
    || echo "（[web,pty] 安装失败，dashboard 可能无法启动；可手动执行：cd $HERMES_DIR && source venv/bin/activate && python -m pip install -e \".[web,pty]\"）"
else
  echo "未找到 Hermes venv（$HERMES_DIR），跳过 web extra 安装"
fi

echo "==> 验证安装..."
if command -v hermes >/dev/null 2>&1; then
  hermes --version 2>/dev/null || echo "（hermes 已安装，--version 暂未输出）"
else
  echo "未在当前 PATH 检测到 hermes 命令；官方脚本已将其加入 shell 配置，请重开终端或 source 对应 rc 文件。"
fi
echo "==> Hermes Agent 安装完成"
