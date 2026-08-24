#!/usr/bin/env bash
# ============================================================================
# bootstrap-dsh.sh — 在 WSL 发行版内安装 DeepSeek Harness（dsh）
# ----------------------------------------------------------------------------
# 由 WebUI Shell 在 WSL 内通过 /bin/bash 执行（脚本写入临时文件后运行）。
# 输出逐行回传桌面壳日志面板。
#
# 设计原则：
#   - 不锁版本：dsh 始终安装最新版（npm install -g @deepseek-ai/dsh）
#   - 不锁 Node 主版本：只要求满足官方支持的版本范围
#     （官方要求 Node.js 22.19+ 或 24+）
#   - 尽可能多的发行版支持：Ubuntu/Debian、Fedora、RHEL/CentOS/Rocky/Alma、
#     Arch/Manjaro、openSUSE、Alpine；未知发行版给出明确提示而非静默失败
#   - 通过 npm 全局安装 dsh，使 `dsh` 命令可用于后续启动（autostart.cmd: dsh web）
# ============================================================================
set -eu
# pipefail 仅 bash 支持；在 sh/dash 下跳过，避免 "set: pipefail: invalid option name"
if [ -n "${BASH_VERSION:-}" ]; then set -o pipefail; fi

echo "==> 检测 Linux 发行版..."
. /etc/os-release 2>/dev/null || { echo "无法读取 /etc/os-release" >&2; exit 1; }
DISTRO="${ID:-unknown}"
DISTRO_LIKE="${ID_LIKE:-}"
echo "    发行版: ${DISTRO} (like: ${DISTRO_LIKE})"

# 版本比较：a >= b（点分版本），如 24.1.0 >= 24.0.0
ver_ge() {
  [ "$#" -eq 2 ] || return 2
  local a="$1" b="$2"
  IFS='.' read -r -a A <<< "$a"
  IFS='.' read -r -a B <<< "$b"
  local i
  for i in 0 1 2; do
    local av="${A[$i]:-0}" bv="${B[$i]:-0}"
    av=$((10#$av)); bv=$((10#$bv))
    if [ "$av" -gt "$bv" ]; then return 0; fi
    if [ "$av" -lt "$bv" ]; then return 1; fi
  done
  return 0
}

# 判断 Node 版本是否满足官方要求：^22.19.0 || >=24.0.0
# ^22.19.0 表示 >=22.19.0 且 <23.0.0；或 >=24.0.0。23.x 不在支持范围。
node_ok() {
  [ -x "$(command -v node)" ] || return 1
  local v
  v="$(node -v 2>/dev/null | tr -d 'v')" || return 1
  # >=22.19.0 且 <23.0.0
  if ver_ge "$v" 22.19.0 && ! ver_ge "$v" 23.0.0; then return 0; fi
  # 或 >=24.0.0
  if ver_ge "$v" 24.0.0; then return 0; fi
  return 1
}

install_node() {
  echo "==> 安装 Node.js（官方要求 ^22.19.0 || >=24.0.0）..."
  case "$DISTRO" in
    ubuntu|debian)
      # NodeSource 官方源，安装 Node 24（LTS，满足 >=24）
      curl -fsSL https://deb.nodesource.com/setup_24.x | bash -
      apt-get update && apt-get install -y nodejs
      ;;
    fedora)
      # Fedora 的 nodejs24 模块
      if command -v dnf >/dev/null 2>&1; then
        dnf module enable -y nodejs:24 2>/dev/null || true
        dnf install -y nodejs npm || dnf install -y nodejs24 npm
      else
        echo "未找到 dnf" >&2; exit 1
      fi
      ;;
    centos|rhel|rocky|almalinux)
      # NodeSource RPM 源
      curl -fsSL https://rpm.nodesource.com/setup_24.x | bash -
      yum install -y nodejs
      ;;
    arch|manjaro)
      pacman -Sy --noconfirm nodejs npm
      ;;
    opensuse|opensuse-leap|opensuse-tumbleweed|suse)
      if command -v zypper >/dev/null 2>&1; then
        zypper --non-interactive install nodejs24 || zypper --non-interactive install nodejs npm
      else
        echo "未找到 zypper" >&2; exit 1
      fi
      ;;
    alpine)
      apk add --no-cache nodejs npm
      ;;
    *)
      # 兜底：尝试按 ID_LIKE 处理
      case "$DISTRO_LIKE" in
        *debian*|*ubuntu*) curl -fsSL https://deb.nodesource.com/setup_24.x | bash -; apt-get update && apt-get install -y nodejs ;;
        *fedora*|*rhel*|*centos*) curl -fsSL https://rpm.nodesource.com/setup_24.x | bash -; yum install -y nodejs ;;
        *arch*) pacman -Sy --noconfirm nodejs npm ;;
        *) echo "不支持的发行版: ${DISTRO}，请手动安装 Node.js 22.19+ 或 24+，然后重新运行本脚本。" >&2; exit 1 ;;
      esac
      ;;
  esac
}

if node_ok; then
  echo "    Node 已满足要求，跳过安装（当前 $(node -v)）"
else
  install_node
fi

echo "==> 验证 Node.js: $(node -v) / npm $(npm -v)"

# npm 镜像源：国内网络下载慢，默认使用 npmmirror 镜像加速。
# 如需官方源，可通过环境变量 NPM_REGISTRY 覆盖，例如：
#   NPM_REGISTRY=https://registry.npmjs.org/ npm install ...
NPM_REGISTRY="${NPM_REGISTRY:-https://registry.npmmirror.com/}"

echo "==> 全局安装 DeepSeek Harness（最新版，不锁版本，registry=${NPM_REGISTRY}）..."
npm install -g "@deepseek-ai/dsh" --registry="$NPM_REGISTRY"

echo "==> 验证安装..."
dsh --version

echo "==> DeepSeek Harness 安装完成"
