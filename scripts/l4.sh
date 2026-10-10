#!/usr/bin/env bash
# **★ L4（Colab CLI）会话的一键准备与操作 ✗ ★**（**第 492 轮 ✓；**用户要求「做成脚本 ✓」）
#
# **∴ 它解决什么 ✗**：**每次连 L4 都要**手打一长串 ssh ＋ ProxyCommand**✗
#   ⇒ **∴ 而且**环境准备**要**记一堆命令**（**Rust／Vulkan ICD／clone／build ✓）
#     ＋ **∴ 而**最容易踩的两个坑**✗：
#       **∴ ①** **没有 `nvidia_icd.json` ⇒ wgpu **回退 `llvmpipe`**✗
#         ⇒ **∴ 于是**：**把**软件渲染**当成 L4** ✓（**第 318 轮踩过 ✓）★**** ✓✓
#       **∴ ②** `pkill -f yanshi-serve` **会**匹配到自己所在的 ssh 命令**✗
#         ⇒ **∴ 远端 shell**自杀 ⇒ **∴ 输出**全空** ✓（**本轮踩过 ✓）
#           ＋ **∴ 所以**本脚本**一律用 `pkill -x`** ✓ ★**** ✓✓
#
# **∴ 用法 ✗**：
#   scripts/l4.sh status              # 看会话 ＋ nvidia-smi ＋ vulkaninfo 的适配器名
#   scripts/l4.sh up                  # 一键准备：Rust ＋ Vulkan ICD ＋ clone ＋ 带 gpu 构建
#   scripts/l4.sh run '<命令>'         # 在 L4 上跑一条命令（非交互，走 ssh）
#   scripts/l4.sh sync                # 把**已推送的**本地 HEAD 同步到 L4（走 GitHub ✓）
#   scripts/l4.sh accounts [--rounds N] [--width W] [--height H]   # 在 L4 上量两本账
#   scripts/l4.sh down                # 停掉会话（释放配额 ✓）
#   scripts/l4.sh sh                  # 交互式 shell（需要 TTY ✓）
#
# **∴ 环境变量 ✗**：`L4_SESSION`（缺省 `dawang`）／`L4_DIR`（缺省 `/content/yanshi`）
#
# **∴ 两面（**AGENTS.md 第 3 条 ✓）★**：
#   **∴ 收益 ✗**：**一条命令**从零到能测**✗
#     ＋ **∴ 且**：**把**适配器名**与**自杀陷阱**都固化在脚本里** ✓ ★**** ✓✓
#   **∴ 代价 ✗**：**多一层包装**✗ ⇒ **∴ 而**它**依赖 colab CLI 的内部约定**（`root@colab-runtime` ✓）
#     ＋ **∴ 若** CLI 改了那个抽象主机名**✗ ⇒ **∴ 本脚本**要跟着改** ✓
#       ＋ **∴ 且**：**up 会**装包**（**Rust ＋ NVIDIA 用户态库 ✓）⇒ **∴ 在别人机器上跑前**要知道这一点** ✓ ★**** ✓✓
#
# **∴ 退出码 ✗**：0 成功｜1 远端命令失败｜2 用法错｜**3 前提不满足**（**如没有 colab CLI ✓）

set -uo pipefail

SESSION="${L4_SESSION:-dawang}"
L4_DIR="${L4_DIR:-/content/yanshi}"
REPO_SLUG="gradetwo/yanshi"

# **∴ colab CLI 在 `~/.local/bin` ✗** ⇒ **∴ 它**常常不在 PATH 里** ✓（**本轮实测 ✓）
export PATH="${HOME}/.local/bin:${PATH}"

die() { printf '✗ %s\n' "$*" >&2; exit "${2:-1}"; }

command -v colab >/dev/null 2>&1 || die "找不到 colab CLI ⇒ 装：uv tool install google-colab-cli（或把它加进 PATH）" 3
[ -x "${HOME}/.local/bin/colab" ] || true

# **∴ 密钥：取第一个存在的 ✓**（**与 CLI 的缺省一致 ✓）
IDENTITY=""
for k in "${HOME}/.ssh/id_ed25519" "${HOME}/.ssh/id_ecdsa"; do
  [ -f "${k}" ] && { IDENTITY="${k}"; break; }
done
[ -n "${IDENTITY}" ] || die "找不到 ssh 私钥（id_ed25519／id_ecdsa）" 3

# **∴ 抽象主机名是 CLI 的内部约定 ✗**（**`ssh.py` 的 `_SSH_HOST` ✓）
L4_HOST="root@colab-runtime"

# **∴ 非交互 ssh ✓**：BatchMode ⇒ 绝不卡在密码提示 ✓
l4_ssh() {
  ssh -o "ProxyCommand=colab ssh --proxy-mode -s ${SESSION} --identity ${IDENTITY}" \
      -o StrictHostKeyChecking=no \
      -o UserKnownHostsFile=/dev/null \
      -o LogLevel=ERROR \
      -o BatchMode=yes \
      -o ConnectTimeout=60 \
      -i "${IDENTITY}" \
      "${L4_HOST}" "$@"
}

# **★ 加重试 ✗ ★**（**第 492 轮实测的一条**硬约束 ✓）：
#   **∴ 症状 ✗**：`[colab] Already-active SSH session (HTTP 429)`**✗
#     ⇒ **∴ 原因**：**colab 的 ssh 通道**一次只允许**一个连接** ✓
#       ＋ **∴ 所以**：**紧接的第二次／第三次 ssh**会被拒** ✓ ★**** ✓✓
#   **∴ 两条对策（**都要 ✓）**：
#     **∴ ①** **把远端检查**合并进**一次 ssh**✗（**见 `cmd_status` ✓）
#       ＋ **∴ ②** **本函数**遇 429 就**等一下重试**（**最多 5 次，间隔递增 ✓）★**** ✓✓
l4_ssh_retry() {
  local i out rc
  for i in 1 2 3 4 5; do
    out="$(l4_ssh "$@" 2>&1)"; rc=$?
    case "${out}" in
      *"Already-active SSH session"*|*"HTTP 429"*)
        printf '  · ssh 通道占用（429）⇒ %d 秒后重试（第 %d 次）\n' "$((i * 3))" "${i}" >&2
        sleep $((i * 3))
        continue
        ;;
    esac
    printf '%s\n' "${out}"
    return "${rc}"
  done
  printf '%s\n' "${out}"
  return "${rc}"
}

# **∴ 交互式：要 `-t` ✓**
l4_ssh_tty() {
  ssh -t -o "ProxyCommand=colab ssh --proxy-mode -s ${SESSION} --identity ${IDENTITY}" \
      -o StrictHostKeyChecking=no \
      -o UserKnownHostsFile=/dev/null \
      -o LogLevel=ERROR \
      -i "${IDENTITY}" \
      "${L4_HOST}" "$@"
}

cmd_status() {
  echo "== 会话（本地查 ✓）=="
  colab sessions 2>&1 | grep -a "${SESSION}" || echo "  （没有名为 ${SESSION} 的会话 ⇒ 先跑 scripts/l4.sh up ✓）"
  echo ""
  # **★ 远端检查**合并成一次 ssh ✗ ★**（**第 492 轮实测 ✓）：
  #   **∴ 为什么 ✗**：**colab 的 ssh 通道**一次只允许一个连接**✗
  #     ⇒ **∴ 原来**分三次 ssh ⇒ **∴ 第三次**被 429 拒** ✓ ★**** ✓✓
  echo "== GPU ＋ Vulkan 适配器 ＋ 仓库（**一次 ssh 问完 ✓）=="
  l4_ssh_retry 'echo "-- GPU："; nvidia-smi --query-gpu=name,driver_version,memory.total --format=csv,noheader | sed "s/^/   /"
echo "-- Vulkan ICD："; ls /usr/share/vulkan/icd.d/ 2>/dev/null | sed "s/^/   /" || echo "   ✗ 没有 ICD ⇒ wgpu 会回退 llvmpipe"
echo "-- 适配器（**必须含 NVIDIA ✓）："; timeout 120 vulkaninfo --summary 2>/dev/null | grep -aE "deviceName|driverName|driverInfo" | sed "s/^/   /"
echo "-- 仓库与产物："; cd '"${L4_DIR}"' 2>/dev/null && { git log --oneline -1 | sed "s/^/   /"; ls -la target/release/yanshi-serve 2>/dev/null | sed "s/^/   /" || echo "   （还没构建 ✓）"; } || echo "   （还没 clone ✓）"' 2>&1 | sed 's/^/  /'
}

cmd_up() {
  echo "== 1/5 会话 =="
  colab sessions 2>&1 | grep -a "${SESSION}" >/dev/null || {
    echo "  创建会话 ${SESSION}（--gpu L4）"
    colab new -s "${SESSION}" --gpu L4 2>&1 | tail -3
  }
  colab sessions 2>&1 | grep -a "${SESSION}" | sed 's/^/  /'

  # **★ 步骤 2–4 **合并成一次 ssh**✗ ★**（**第 492 轮实测：**分次会被 429 拒 ✓）
  echo "== 2/3 Rust ＋ Vulkan ＋ 仓库（**一次 ssh ✓）=="
  l4_ssh_retry 'set -u
export PATH=$HOME/.cargo/bin:$PATH
# **∴ ① Rust（**已有则跳过 ✓）
if command -v cargo >/dev/null; then echo "   Rust：已有 $(cargo --version)"; else
  curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup.sh && sh /tmp/rustup.sh -y --profile minimal --default-toolchain stable >/tmp/rustup.log 2>&1
  export PATH=$HOME/.cargo/bin:$PATH
  echo "   Rust：装好 $(cargo --version)"
fi
# **∴ ② Vulkan ICD（**必需 ✗ —— 否则 wgpu 回退 llvmpipe ✓）
if [ -f /usr/share/vulkan/icd.d/nvidia_icd.json ]; then echo "   Vulkan：ICD 已在 ✓"; else
  apt-get install -y libnvidia-gl-580 vulkan-tools >/tmp/apt-vulkan.log 2>&1 && echo "   Vulkan：已装（$(ls /usr/share/vulkan/icd.d/)）" || { echo "   ✗ Vulkan 装失败 ⇒ 看 /tmp/apt-vulkan.log"; tail -5 /tmp/apt-vulkan.log; }
fi
timeout 120 vulkaninfo --summary 2>/dev/null | grep -aE "deviceName|driverInfo" | sed "s/^/   /"
# **∴ ③ 仓库（**HTTPS ✓ —— L4 上没有 GitHub 密钥 ✓）
if [ -d '"${L4_DIR}"'/.git ]; then cd '"${L4_DIR}"' && git fetch -q origin && git reset -q --hard origin/main && echo "   仓库：$(git log --oneline -1)"; else
  git clone -q https://github.com/'"${REPO_SLUG}"'.git '"${L4_DIR}"' && cd '"${L4_DIR}"' && echo "   仓库：$(git log --oneline -1)"
fi' 2>&1 | sed 's/^/  /'

  echo "== 3/3 构建（**--features gpu ✓，后台 ✓）=="
  l4_ssh_retry "cd ${L4_DIR} && export PATH=\$HOME/.cargo/bin:\$PATH && nohup cargo build --release -p yanshi-http --bin yanshi-serve --features gpu > /tmp/build-gpu.log 2>&1 & sleep 5; echo '  已启动（日志 /tmp/build-gpu.log ✓）'" 2>&1 | sed 's/^/  /'
  echo ""
  echo "⇒ 之后用 scripts/l4.sh status 看结果，或 scripts/l4.sh wait-build 等构建完成 ✓"
}

cmd_wait_build() {
  echo "== 等构建完成（**最多 40 分钟 ✓）=="
  l4_ssh 'for i in $(seq 1 240); do if pgrep -x cargo >/dev/null; then sleep 10; else break; fi; done; tail -3 /tmp/build-gpu.log; ls -la '"${L4_DIR}"'/target/release/yanshi-serve 2>/dev/null || echo "  ✗ 没有产物 ⇒ 看 /tmp/build-gpu.log"' 2>&1 | sed 's/^/  /'
}

cmd_run() {
  [ $# -ge 1 ] || die "用法：scripts/l4.sh run '<命令>'" 2
  l4_ssh "$@"
}

cmd_sync() {
  # **∴ 走 GitHub ✗** ⇒ **∴ 不依赖 scp／rsync**，且**用已推送的提交** ✓
  local head
  head="$(git rev-parse --short HEAD)"
  local remote_head
  remote_head="$(timeout 60 git ls-remote origin -h refs/heads/main 2>/dev/null | cut -f1 | cut -c1-7)"
  [ "${head}" = "${remote_head}" ] || die "本地 HEAD（${head}）与远端（${remote_head}）不一致 ⇒ 先 push（**不许**测一份未推送的代码 ✓）" 2
  l4_ssh "cd ${L4_DIR} && git fetch -q origin && git reset -q --hard origin/main && git log --oneline -1 && echo '工作区：'\$(git status --porcelain | wc -l)' 项'" 2>&1 | sed 's/^/  /'
}

cmd_accounts() {
  local rounds="${1:-3}" width="${2:-3840}" height="${3:-2160}"
  # **∴ 先确保构建好 ✓**
  l4_ssh "cd ${L4_DIR} && ls target/release/yanshi-serve >/dev/null 2>&1 || { export PATH=\$HOME/.cargo/bin:\$PATH; cargo build --release -p yanshi-http --bin yanshi-serve --features gpu; }" >/dev/null 2>&1
  # **∴ 用仓库自带的工具 ✓**（**它读 `/proc`，Linux 上正好 ✓）
  l4_ssh "cd ${L4_DIR} && pkill -9 -x yanshi-serve 2>/dev/null; sleep 1; node --version >/dev/null 2>&1 || { echo '  ✗ L4 上没有 node ⇒ 装：apt-get install -y nodejs npm'; exit 1; }; timeout 1500 node scripts/tool-render-cost-accounts.mjs --spawn --rounds ${rounds} --baseline target/release/yanshi-serve --compare 'CPU=--gpu off' --compare 'GPU=--gpu on' 2>&1 | tail -18" 2>&1 | sed 's/^/  /'
}

cmd_down() {
  echo "== 停掉会话 ${SESSION}（释放配额 ✓）=="
  colab stop -s "${SESSION}" 2>&1 | tail -3 | sed 's/^/  /'
}

cmd_sh() {
  echo "== 交互式 shell（**在 L4 的 ${L4_DIR} ✓；**退出即断开 ✓）=="
  l4_ssh_tty "cd ${L4_DIR} 2>/dev/null; exec \${SHELL:-/bin/bash} -l"
}

case "${1:-}" in
  status)      shift; cmd_status "$@" ;;
  up)          shift; cmd_up "$@" ;;
  wait-build)  shift; cmd_wait_build "$@" ;;
  run)         shift; cmd_run "$@" ;;
  sync)        shift; cmd_sync "$@" ;;
  accounts)    shift; cmd_accounts "$@" ;;
  down)        shift; cmd_down "$@" ;;
  sh|shell)    shift; cmd_sh "$@" ;;
  ""|-h|--help) sed -n '2,40p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//' ;;
  *) die "未知子命令：$1（用 --help 看用法）" 2 ;;
esac
