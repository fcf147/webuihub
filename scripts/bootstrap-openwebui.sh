#!/usr/bin/env bash
# ============================================================================
# bootstrap-openwebui.sh — 在 WSL 发行版内安装 Open WebUI
# ----------------------------------------------------------------------------
# 由 WebUI Shell 在 WSL 内通过 /bin/bash 执行（脚本写入临时文件后运行）。
# 输出逐行回传桌面壳日志面板。
#
# 设计原则（与 bootstrap-dsh.sh 保持一致）：
#   - 不锁版本：始终安装最新版（pip install open-webui）
#   - 多发行版支持：Ubuntu/Debian、Fedora、RHEL/CentOS/Rocky/Alma、
#     Arch/Manjaro、openSUSE、Alpine；未知发行版给出明确提示
#   - 通过 pip 安装 open-webui，启动命令 `open-webui serve`（默认端口 8080）
# ============================================================================
set -eu
if [ -n "${BASH_VERSION:-}" ]; then set -o pipefail; fi

echo "==> 检测 Linux 发行版..."
. /etc/os-release 2>/dev/null || { echo "无法读取 /etc/os-release" >&2; exit 1; }
DISTRO="${ID:-unknown}"
DISTRO_LIKE="${ID_LIKE:-}"
echo "    发行版: ${DISTRO} (like: ${DISTRO_LIKE})"

# 判断 Python3 + venv + pip 是否就绪
py_ok() {
  command -v python3 >/dev/null 2>&1 || return 1
  python3 -c "import venv" 2>/dev/null || return 1
  return 0
}

install_python() {
  echo "==> 安装 Python3 / venv / pip..."
  case "$DISTRO" in
    ubuntu|debian)
      apt-get update && apt-get install -y python3 python3-venv python3-pip
      ;;
    fedora)
      dnf install -y python3 python3-venv python3-pip
      ;;
    centos|rhel|rocky|almalinux)
      # EPEL 提供 python3-pip
      dnf install -y epel-release 2>/dev/null || true
      dnf install -y python3 python3-venv python3-pip 2>/dev/null || \
        yum install -y epel-release python3 python3-pip 2>/dev/null || \
        { echo "    警告: 无法通过 EPEL 安装 python3-pip，请手动安装后重试。" >&2; exit 1; }
      ;;
    arch|manjaro)
      pacman -Sy --noconfirm python python-pip
      ;;
    opensuse*|suse)
      zypper --non-interactive install python3 python3-pip
      ;;
    alpine)
      apk add --no-cache python3 py3-pip
      ;;
    *)
      case "$DISTRO_LIKE" in
        *debian*|*ubuntu*) apt-get update && apt-get install -y python3 python3-venv python3-pip ;;
        *fedora*|*rhel*|*centos*) dnf install -y python3 python3-venv python3-pip || yum install -y python3 python3-pip ;;
        *arch*) pacman -Sy --noconfirm python3 python-pip ;;
        *) echo "不支持的发行版: ${DISTRO}，请手动安装 Python3.11+ 与 pip，然后重新运行本脚本。" >&2; exit 1 ;;
      esac
      ;;
  esac
}

if py_ok; then
  echo "    Python 已就绪（$(python3 --version 2>&1)）"
  PY_VER="$(python3 -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")' 2>/dev/null)"
  if python3 -c "import sys; exit(0 if sys.version_info >= (3,11) else 1)" 2>/dev/null; then
    echo "    Python 版本 ${PY_VER} ✓（需要 ≥3.11）"
  else
    echo "    错误: Python 版本 ${PY_VER} 过低，Open WebUI 需要 Python 3.11+" >&2
    exit 1
  fi
else
  install_python
  # 安装后再次验证版本
  PY_VER="$(python3 -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")' 2>/dev/null)"
  if ! python3 -c "import sys; exit(0 if sys.version_info >= (3,11) else 1)" 2>/dev/null; then
    echo "    错误: 安装后 Python 版本仍为 ${PY_VER}，需要 ≥3.11。请手动安装 Python 3.11+ 后重试。" >&2
    exit 1
  fi
  echo "    Python 版本 ${PY_VER} ✓（需要 ≥3.11）"
fi

# 为隔离依赖，使用独立的 venv（避免污染系统 site-packages）。
VENV_DIR="${VENV_DIR:-$HOME/.venv/open-webui}"
echo "==> 创建虚拟环境: ${VENV_DIR}"
python3 -m venv "$VENV_DIR"
# shellcheck disable=SC1091
. "$VENV_DIR/bin/activate"

# pip 镜像源：国内网络下载慢，默认使用清华镜像加速。
PIP_INDEX="${PIP_INDEX:-https://pypi.tuna.tsinghua.edu.cn/simple/}"

echo "==> 升级 pip 并安装 Open WebUI（最新版，不锁版本，index=${PIP_INDEX}）..."
python3 -m pip install --upgrade pip
python3 -m pip install --index-url "$PIP_INDEX" open-webui

echo "==> 验证安装..."
if test -f "$VENV_DIR/bin/open-webui"; then
  echo "    open-webui 可执行文件存在（$VENV_DIR/bin/open-webui）"
else
  echo "    警告: 未找到 open-webui 可执行文件，请检查安装日志" >&2
fi

echo "==> Open WebUI 安装完成"
echo "    启动命令（由壳 autostart 调用）: $VENV_DIR/bin/open-webui serve --port 8080"
