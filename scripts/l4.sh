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
#   scripts/l4.sh bg <名字> '<命令>'    # 起一个**脱离连接**的作业（写 log／rc／pid ✓）
#   scripts/l4.sh tnew <名字> '<命令>'  # **tmux 作业**（**推荐 ✓**：一次起 ⇒ 之后 tlog 抓输出 ✓）
#   scripts/l4.sh tlog <名字> [行数]     # 抓该作业的屏幕尾部（**短 ssh ⇒ 不阻塞 ✓**）
#   scripts/l4.sh tls                    # 列出所有 tmux 作业
#   scripts/l4.sh tkill <名字>           # 结束并销毁该作业
#   scripts/l4.sh tsend <名字> '<命令>'  # **向该作业的窗口**发命令**（**不必重连 ✓）
#   scripts/l4.sh twin <会话> <窗口名> '<命令>'  # **同一会话开新窗口**（**每项工作一个窗口 ⇒ 互不干扰 ✓）
#   scripts/l4.sh tlog <名字>[:<窗口>] [行数]    # 抓某一窗口的屏幕（**短 ssh ✓）
#   scripts/l4.sh jobs                # 列作业 ＋ 各自退出码（**机器可读 ⇒ 能判红绿 ✓）
#   scripts/l4.sh tail <名字> [行数]    # 看作业输出
#   scripts/l4.sh rc <名字>            # 只看退出码（**0 才是绿 ✓）
#   scripts/l4.sh sh                  # 交互式 shell（需要 TTY ✓）
#
# **∴ 环境变量 ✗**：`L4_SESSION`（缺省 `dawang`）／`L4_DIR`（缺省 `/content/yanshi`）／**`L4_BRANCH`（缺省 `l4-sync` ✓）**
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
# **★ 专用同步分支 ✗ ★**（**第 606 轮 ✓；**用户建议 ✓）：**L4 只拉这个分支**✗
#   **∴ 为什么 ✗**：**原来**必须推 `main` 才能同步**✗
#     ⇒ **∴ 于是**：**实验代码**会进 main** ＋ **每轮都要等 main 的 CI** ✓
#       ＋ **∴ 现在 ✗**：**L4 侧只要 `git pull`（**不换分支 ✓）**
#         ＋ **∴ 而** main **保持干净**（**∴ 验证通过后再合并 ✓）★
L4_BRANCH="${L4_BRANCH:-l4-sync}"
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
  # **★★★ 会话必须**真的是 GPU 变体**✗ ★★★**（**第 578 轮实测 ✓；**用户提示 ✓）
  #   **∴ 两个坑叠加 ✗**：
  #     **∴ ①** `colab new … --gpu L4` **只在会话**不存在**时才跑**✗
  #       ⇒ **∴ 于是**：**同名会话一旦存在（**哪怕是旧的 CPU 会话 ✓）
  #         ⇒ **∴ `--gpu L4` **静默不生效**** ✓（**第 576 轮实测 ✓）
  #     **∴ ②** **L4 空闲会被**回收**✗ ⇒ **∴ 回收后 `colab` **自动重建为 CPU 实例**** ✓
  #       ⇒ **∴ 症状 ✗**：**`Hardware: CPU | Variant: DEFAULT`**✗
  #         ＋ **∴ 而**它**仍然装了 `nvidia_icd.json`** ✗
  #           ⇒ **∴ 于是**：**wgpu**回退 `llvmpipe`**✗
  #             ⇒ **★ 若不检查，就会把**软件渲染**当成 L4**** ✓（**第 318 轮的坑 ✓）★
  #   **∴ 所以 ✗**：**每次 `up` 都**校验 `Variant: GPU`**✗
  #     ⇒ **∴ 不是 GPU ⇒ **停掉 ＋ 用 `--gpu L4` 重建**** ✓ ★
  local line
  line="$(colab sessions 2>&1 | grep -a "${SESSION}" | head -1 || true)"
  if [ -z "$line" ] || ! printf '%s' "$line" | grep -aq 'Variant: GPU'; then
    if [ -n "$line" ]; then
      echo "  ★ 现有会话不是 GPU 变体 ⇒ 停掉重建"
      echo "    ${line}"
      colab stop -s "${SESSION}" >/dev/null 2>&1 || true
      sleep 4
    else
      echo "  没有会话 ⇒ 新建"
    fi
    echo "  创建会话 ${SESSION}（--gpu L4）"
    colab new -s "${SESSION}" --gpu L4 2>&1 | tail -3
    sleep 5
  fi
  colab sessions 2>&1 | grep -a "${SESSION}" | sed 's/^/  /' || true
  # **★ 硬校验：拿不到 GPU 变体就**直接失败**（**不许拿 llvmpipe 凑数 ✓）★**
  colab sessions 2>&1 | grep -a "${SESSION}" | grep -aq 'Variant: GPU' || {
    echo "  ✗ 会话 ${SESSION} 不是 GPU 变体（Hardware/Variant 见上）" >&2
    echo "    ⇒ 不继续。请检查配额，或换 --gpu 变体（T4／L4／G4／H100／A100）" >&2
    return 1
  }

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
# **∴ ③ node（**测量工具要用 ✓）✗**（**第 501 轮 ✓）
  #   **∴ 为什么 ✗**：**colab 的镜像**不保证带 node** ✓
  #     ⇒ **∴ 实测 ✗**：**新实例**没有 ⇒ `node: command not found`** ✓
  #       ＋ **∴ 而** `tool-render-cost-accounts.mjs` 等**都靠 node** ✓
  #         ⇒ **∴ 所以**：**在这里**装上**✗ ⇒ **∴ 下次重建**自动带上** ✓ ★**** ✓✓
  #   **∴ 必须带 `-y` ✗**：**否则 apt 会**等交互确认**✗
  #     ⇒ **∴ 我**上轮**漏了它**✗ ⇒ **∴ 作业**永远显示"运行中"** ✓（**实测 ✓）★**** ✓✓
  if command -v node >/dev/null 2>&1; then echo "   node：已有 $(node --version)"; else
    DEBIAN_FRONTEND=noninteractive apt-get install -y nodejs >/tmp/node-i.log 2>&1 && echo "   node：装好 $(node --version)" || { echo "   ✗ node 装失败 ⇒ 看 /tmp/node-i.log"; tail -3 /tmp/node-i.log; }
  fi
  # **∴ ④ 仓库（**HTTPS ✓ —— L4 上没有 GitHub 密钥 ✓）
if [ -d '"${L4_DIR}"'/.git ]; then cd '"${L4_DIR}"' && git fetch -q origin && git reset -q --hard origin/'"${L4_BRANCH}"' && echo "   仓库：$(git log --oneline -1)"; else
  git clone -q https://github.com/'"${REPO_SLUG}"'.git '"${L4_DIR}"' && cd '"${L4_DIR}"' && { git checkout -q '"${L4_BRANCH}"' 2>/dev/null || echo "   （${L4_BRANCH} 不存在 ⇒ 留在缺省分支）"; } && echo "   仓库：$(git log --oneline -1)"
fi' 2>&1 | sed 's/^/  /'

  echo "== 3/3 构建（**--features gpu ✓，后台 ✓）=="
  l4_ssh_retry "cd ${L4_DIR} && export PATH=\$HOME/.cargo/bin:\$PATH && nohup cargo build --release -p yanshi-http --bin yanshi-serve --features gpu > /tmp/build-gpu.log 2>&1 & sleep 5; echo '  已启动（日志 /tmp/build-gpu.log ✓）'" 2>&1 | sed 's/^/  /'
  echo ""
  echo "⇒ 之后用 scripts/l4.sh status 看结果，或 scripts/l4.sh wait-build 等构建完成 ✓"
}

cmd_wait_build() {
  # **★★★ 为什么改成**短查询 ＋ 本地循环** ✗ ★★★**（**第 639 轮，用户提问驱动 ✓）
  #   **∴ 原来 ✗**：**一次 ssh 里跑**最多 40 分钟的前台轮询**✗
  #     ＋ **∴ 于是**：**它自己**长时间持有 ssh 会话**✗
  #       ＋ **∴ 而** colab-cli 同一 runtime **只允许一个 ssh 会话**✗
  #         ⇒ **∴ 结果**：**期间**其它 `l4.sh` 命令全部 `HTTP 429`** ✓
  #           ＋ **∴ 而**我上轮**误判成** `up` **持有**✗（**∴ 其实 `up` 的构建走 `nohup &` ⇒ 早就 detach ✓）★
  #   **∴ 现在 ✗**：**每次只发一次**亚秒级查询**✗ ＋ **本地 `sleep`** ✓
  #     ＋ **∴ 从而**：**既不长时间占用会话，也能等完构建** ✓ ★
  local rounds="${L4_WAIT_ROUNDS:-80}"
  local gap="${L4_WAIT_GAP:-30}"
  echo "== 等构建完成（最多 $((rounds*gap/60)) 分钟；每 $gap 秒一次短查询）=="
  local i
  for i in $(seq 1 "$rounds"); do
    if ! l4_ssh 'pgrep -x cargo >/dev/null' >/dev/null 2>&1; then
      echo "  ✓ 构建已结束（第 ${i} 次查询）"
      break
    fi
    sleep "$gap"
  done
  l4_ssh 'tail -3 /tmp/build-gpu.log 2>/dev/null; ls -la '"${L4_DIR}"'/target/release/yanshi-serve 2>/dev/null | tail -1' 2>&1 | sed 's/^/  /'
}
cmd_run() {
  [ $# -ge 1 ] || die "用法：scripts/l4.sh run '<命令>'" 2
  l4_ssh "$@"
}

# **★★★ tmux 作业：一次起 ⇒ 之后只用**短 ssh 抓输出** ✗ ★★★**（**第 608 轮 ✓；**用户建议 ✓）
#   **∴ 为什么需要 ✗**：**原来只有 `l4.sh run '<命令>'`**✗
#     ⇒ **∴ 于是**：**①** 换一项工作就要**重发 ssh**（**还容易撞 `HTTP 429`** ✓）
#       ＋ **∴ ②** 长任务**阻塞**调用（**靠 timeout ✓）
#         ＋ **∴ ③** 多项工作**不能并发** ✓
#   **∴ 现在 ✗**：**在 L4 上用 `tmux` 起**脱离连接的作业**
#     ＋ **∴ 输出**落在 `/tmp/<名字>.log`（**且可用 `capture-pane` 直接抓屏 ✓）
#       ⇒ **∴ 从而**：**不阻塞 ＋ 不互扰 ＋ 少撞 429** ✓ ★
#   **∴ 子命令 ✗**：
#     `l4.sh tnew <名字> '<命令>'`   起一个后台作业（**已存在则报错 ✓）
#     `l4.sh tlog <名字> [行数]`     抓它的屏幕尾部（**短 ssh ✓）
#     `l4.sh tls`                    列出所有作业
#     `l4.sh tkill <名字>`           结束并销毁该作业
#   **∴ 代价（**两面 ✓）✗**：**要记名字**✗ ＋ **∴ 且**作业**不随 ssh 结束而自动清理**（**∴ 用 `tkill` 或 `down` ✓）★
cmd_tnew() {
  local name="${1:-}"; shift || true
  [ -n "${name}" ] || die "用法：scripts/l4.sh tnew <名字> '<命令>'" 2
  [ $# -ge 1 ] || die "用法：scripts/l4.sh tnew <名字> '<命令>'" 2
  local cmd="$*" b64_cmd b64_pre
  # **★★★ 两条硬规矩（**都是实测换来的 ✓）★★★**（**第 608 轮 ✓）
  #   **∴ ① 内容走 `base64` ✗**：**否则要经本机→ssh→tmux→bash 四层转义** ✓
  #   **∴ ② 日志用**重定向**✗（`> f 2>&1`）**：**绝不嵌 `| tee`**✗
  #     ＋ **∴ 因为**管道要经引号 ⇒ **∴ 那**正是第一版**会话秒退**的原因** ✓
  #       （**∴ 手动验证过：`> /tmp/t1.log 2>&1` **完全可用** ✓）★
  # **★★★ 第三条硬规矩：`cd` 必须写进**脚本本身** ✗ ★★★**（**第 610 轮实测 ✓）
  #   **∴ 原来的坑 ✗**：**`cd ${L4_DIR}` 只出现在 tmux 命令的外层**✗
  #     ＋ **∴ 而** tmux 起的是**登录 shell** ⇒ **∴ 于是**：**作业的 cwd 是 `/root`** ✓
  #       ⇒ **∴ 症状 ✗**：**`cargo build` 报
  #         「could not find `Cargo.toml` in `/root` or any parent directory」** ✓ ★
  #   **∴ 修法 ✗**：**把 `cd` 放进**写到远端的那份脚本**里**✗
  #     ⇒ **∴ 从而**与**外层 cwd 无关** ✓ ★
  b64_cmd="$(printf '%s' "${cmd}" | base64 -w0)"
  # **★★★ 第四条硬规矩：补 `XDG_RUNTIME_DIR` ✗ ★★★**（**第 612 轮实测 ✓）
  #   **∴ 症状 ✗**：**在 tmux 作业里跑 GPU 测试**报
  #     「**XDG_RUNTIME_DIR is invalid or not set in the environment**」**✗
  #     ＋ **∴ 而**看代码毫无问题**（**∴ 本机与 CI 都过 ✓）
  #       ⇒ **∴ 根因 ✗**：**tmux 起的是**登录 shell 但**没有**桌面会话的环境**✗
  #         ＋ **∴ 而** wgpu 的 Vulkan 后端**要它** ✓
  #           ⇒ **∴ 于是**：**测试**全红**✗ 而**产品没坏** ✓（**∴ 正是第 402 轮那类"判据自己坏了" ✓）★
  #   **∴ 修法 ✗**：**所有 tmux 作业都先**导出**它**（**∴ 固化 ⇒ 以后不必记 ✓）★
  b64_pre="$(printf 'cd %s || exit 9\nexport XDG_RUNTIME_DIR=/tmp/xdg-%s\nmkdir -p "$XDG_RUNTIME_DIR"\nchmod 700 "$XDG_RUNTIME_DIR"\n' "${L4_DIR}" "${name}" | base64 -w0)"
  l4_ssh "printf '%s' '${b64_pre}' | base64 -d > /tmp/${name}.sh; printf '%s' '${b64_cmd}' | base64 -d >> /tmp/${name}.sh" >/dev/null 2>&1
  l4_ssh "tmux has-session -t ${name} 2>/dev/null && { echo '  ✗ 已有同名作业 ⇒ 先 tkill ${name}'; exit 3; }; tmux new-session -d -s ${name} 'bash /tmp/${name}.sh > /tmp/${name}.log 2>&1; echo EXIT=\$? >> /tmp/${name}.log'; sleep 1; tmux ls 2>/dev/null | sed 's/^/  /'" 2>&1 | tail -4
}
# **★ 向作业的窗口发命令 ✗ ★**（**第 608 轮 ✓；**用户提示「窗口输入输出都方便」 ✓）
#   **∴ 用途 ✗**：**不必重连**✗ ⇒ **∴ 直接**把新命令送进那个窗口** ✓
#     ＋ **∴ 与 `tlog` 配对 ✗**：**发完 ⇒ 抓屏**（**∴ 全程短 ssh ✓）★
cmd_tsend() {
  local name="${1:-}"; shift || true
  [ -n "${name}" ] || die "用法：scripts/l4.sh tsend <名字> '<命令>'" 2
  [ $# -ge 1 ] || die "用法：scripts/l4.sh tsend <名字> '<命令>'" 2
  l4_ssh "tmux send-keys -t ${name} $(printf '%q' "$*") Enter && echo '  ✓ 已送进 ${name}'" 2>&1 | tail -2
}

# **★ 在同一会话里**开新窗口** ✗ ★**（**∴ 每项工作一个窗口 ⇒ 互不干扰 ✓）★
cmd_twin() {
  local name="${1:-}"; local win="${2:-}"; shift 2 || true
  [ -n "${name}" ] && [ -n "${win}" ] && [ $# -ge 1 ] || die "用法：scripts/l4.sh twin <会话> <窗口名> '<命令>'" 2
  local b64; b64="$(printf '%s' "$*" | base64 -w0)"
  l4_ssh "printf '%s' '${b64}' | base64 -d > /tmp/${name}_${win}.sh; tmux new-window -t ${name} -n ${win} 'bash /tmp/${name}_${win}.sh 2>&1 | tee /tmp/${name}_${win}.log'; sleep 1; tmux list-windows -t ${name} 2>/dev/null | sed 's/^/  /'" 2>&1 | tail -5
}
cmd_tlog() {
  # **★★★ 优先读**日志文件**，而不是窗口屏 ✗ ★★★**（**第 648 轮实测 ✓）
  #   **∴ 为什么 ✗**：**`tnew` **早已把输出重定向到** `/tmp/<name>.log`**✗
  #     （`tmux new-session -d -s <name> 'bash /tmp/<name>.sh > /tmp/<name>.log 2>&1; …'` ✓）
  #       ＋ **∴ 而**原来 `tlog` **抓的是 `capture-pane`（**窗口屏 ✓）**✗
  #         ⇒ **∴ 于是**作业**一旦结束**（**或屏已滚过 ✓）⇒ **∴ 它**返回空** ✓
  #           ＋ **∴ 我因此**白跑了一轮**（**以为输出丢了 ✓）★
  #   **∴ 现在 ✗**：**先 `cat /tmp/<name>.log`**✗
  #     ＋ **∴ 若**该文件不存在 ⇒ **∴ 再**退回窗口屏 ✓ ★
  local name="$1"
  local lines="${2:-40}"
  [ -n "$name" ] || { die "用法：scripts/l4.sh tlog <名字> [行数]" 2; }
  l4_ssh "if [ -f /tmp/${name}.log ]; then echo '  （来源：/tmp/${name}.log）'; tail -n ${lines} /tmp/${name}.log; else echo '  （没有 /tmp/${name}.log ⇒ 退回窗口屏）'; tmux capture-pane -p -t ${name} 2>/dev/null | grep -v '^$' | tail -n ${lines}; fi" 2>&1
}
cmd_tls() {
  l4_ssh "tmux ls 2>/dev/null || echo '  （没有 tmux 作业）'" 2>&1 | tail -n 12
}

cmd_tkill() {
  local name="${1:-}"
  [ -n "${name}" ] || die "用法：scripts/l4.sh tkill <名字>" 2
  l4_ssh "tmux kill-session -t ${name} 2>/dev/null && echo '  ✓ 已结束 ${name}' || echo '  （没有该作业）'" 2>&1 | tail -2
}

# **★★★ 在 L4 上跑**全量门禁**并回摘要 ✗ ★★★**（**第 608 轮 ✓；**用户要求「优化到脚本里」 ✓）
#   **∴ 为什么 ✗**：**本机跑 cargo 慢**（**≈2–4 分钟起 ✓）
#     ＋ **∴ 而** L4 有 **12 线程**（**实测 clippy 24 s／test 2m33s，并行度 4.1× ✓）
#       ⇒ **∴ 所以**：**重量级一律放 L4** ⇒ **∴ 而**这条子命令**把常用序列封起来** ✓ ★
# **★★★ 廉价健康检查 ✗ ★★★**（**第 618 轮 ✓；**用户「造工具」方法论 ✓）
#   **∴ 为什么需要 ✗**：**L4 会话会**悄悄失效**** ✗：
#     **∴ 症状 ✗**：**`l4.sh run '<命令>'` **不报错、「输出为空」**✗
#       ＋ **∴ 或**者**测量只报 `TypeError: fetch failed` ＋ `SocketError: other side closed`** ✓
#         ⇒ **∴ 于是**：**我**误以为是**产品 bug**✗ ⇒ **∴ 白白排查了好几轮** ✓ ★
#   **∴ 它做什么 ✗**：**一次极短的 ssh**（**只 `echo PONG` ＋ 报 GPU 名** ✓）
#     ＋ **∴ 若**拿不到 PONG ⇒ **∴ 明确说「会话可能已被回收 ⇒ 用 `l4.sh up` 重建」** ✓ ★
#   **∴ 代价 ✗**：**一次 ssh**（**~1–2 秒 ✓）★
# **★★★ up-state：判断 `up` 是否仍在运行 ✗ ★★★**（**第 641 轮 ✓）
#   **∴ 为什么需要它 ✗**：**`pgrep -f 'l4.sh up'` 会**自匹配**✗
#     ＋ **∴ 因为**我自己的检查命令文本里也含这串 ⇒ **∴ 它**永远报"在跑"** ✓
#       ＋ **∴ 代价**：**我因此白等了 4 轮** ✓
#   **∴ 判据改成日志时间戳 ✗**：**静止 ＞ N 秒 ⇒ 认为已结束 ✓
#     ＋ **∴ 并**用 `[l]4.sh` 字符类避自匹配** ✓ ★
cmd_up_state() {
  local log="${L4_UP_LOG:-/tmp/l4up.log}"
  local quiet="${L4_UP_QUIET:-120}"
  if [ ! -f "$log" ]; then
    echo "  （没有日志 ${log}）"
    return 4
  fi
  # **★★★ 看**最新活动**，而不是只看主日志 ✗ ★★★**（**第 642 轮实测修正 ✓）
  #   **∴ 为什么 ✗**：**`up` 把**构建输出**重定向到** `/tmp/build-gpu.log`**✗
  #     ＋ **∴ 于是**主日志**静止**✗ 而**进程**还活着**
  #       ⇒ **∴ 只看主日志 ⇒ **误报"已结束"或"仍在跑"** ✓
  #         ＋ **∴ 正确判据 ✗**：**取**两者中更新的那个** ✓ ★
  local newest="$log"
  if [ -f /tmp/build-gpu.log ] && [ /tmp/build-gpu.log -nt "$newest" ]; then
    newest=/tmp/build-gpu.log
  fi
  local age=$(( $(date +%s) - $(stat -c %Y "$newest") ))
  echo "  （最新活动日志：${newest}）"
  if pgrep -f '[l]4.sh up' >/dev/null 2>&1 || [ "$age" -lt "$quiet" ]; then
    echo "  ⏳ up 可能仍在运行（日志静止 ${age}s）"
    return 0
  fi
  echo "  ✓ up 已结束（日志静止 ${age}s）"
  return 1
}

cmd_ping() {
  local out
  out="$(l4_ssh 'echo PONG; nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1' 2>&1 | tr -d "\r")"
  if printf '%s' "${out}" | grep -q PONG; then
    echo "  ✓ 会话在线：$(printf '%s' "${out}" | grep -v PONG | head -1)"
  else
    # **★★★ 分类，而不是一句"可能已被回收" ✗ ★★★**（**第 647 轮实测 ✓）
    #   **∴ 为什么 ✗**：**原来**唯一 else ⇒ **∴ 于是**把 **429（**被占用 ✓）**和 **404（**已回收 ✓）
    #     **混为一谈**✗ ⇒ **∴ 而**两者**对策相反** ✓
    #       ＋ **∴ 429** ⇒ **会话**活着**✗ 只是**别人连着**（**如 `up` 残留 ✓）⇒ **∴ 等**即可 ✓
    #         ＋ **∴ 404／not found** ⇒ **真的**被回收**✗ ⇒ **∴ 必须**重建** ✓ ★
    if printf '%s' "${out}" | grep -qE '429|Already-active'; then
      echo "  ⏳ 会话**在线，但被另一个 colab ssh 占用**（**429 ✓）"
      echo "     ⇒ **稍后重试**；并可用 \`scripts/l4.sh up-state\` 看 \`up\` 是否仍在跑 ✓"
      printf '%s\n' "${out}" | head -2 | sed 's/^/    /'
      return 5
    fi
    if printf '%s' "${out}" | grep -qiE 'not found|404|does not expose'; then
      echo "  ✗ **会话已被回收**（**404／not found ✓）⇒ 用 \`scripts/l4.sh up\` 重建 ✓"
      printf '%s\n' "${out}" | head -3 | sed 's/^/    /'
      return 4
    fi
    echo "  ⚠ **未知错误**（**既没有 PONG，也没有 429／404 ✓）—— 请**人工判断** ✓"
    printf '%s\n' "${out}" | head -3 | sed 's/^/    /'
    return 6
  fi
}

cmd_gate() {
  local which="${1:-all}"
  l4_ssh "cd ${L4_DIR} && export PATH=\$HOME/.cargo/bin:\$PATH
    echo '── fmt ──'; cargo fmt --all -- --check >/dev/null 2>&1 && echo '  ✓' || echo '  ★ 红'
    if [ '${which}' = all ] || [ '${which}' = clippy ]; then
      echo '── clippy（plain）──'; cargo clippy --workspace --all-targets -- -D warnings 2>&1 | grep -aE '^error' | head -3 || echo '  ✓'
      echo '── clippy（gpu）──'; cargo clippy --workspace --all-targets --features gpu -- -D warnings 2>&1 | grep -aE '^error' | head -3 || echo '  ✓'
    fi
    if [ '${which}' = all ] || [ '${which}' = test ]; then
      echo '── test（plain）──'; cargo test --workspace 2>&1 | grep -aE 'test result: FAILED|^error' | head -3 || echo '  ✓'
      echo '── test（gpu）──'; cargo test --workspace --features gpu 2>&1 | grep -aE 'test result: FAILED|^error' | head -3 || echo '  ✓'
    fi
    echo '（**无 ★ 行 ⇒ 全绿 ✓）'" 2>&1 | tail -n 24
}

# **★★★ 清 L4 上的作业与临时文件 ✗ ★★★**（**用完即清 ⇒ 不占资源 ✓；**配合「用完销毁」的纪律 ✓）★
cmd_clean() {
  l4_ssh "tmux kill-server 2>/dev/null; rm -f /tmp/*.log /tmp/*.sh 2>/dev/null; echo '  ✓ 已清 L4 的 tmux 作业与临时文件'" 2>&1 | tail -2
}

# **★★★ 推送并校验同步分支 ✗ ★★★**（**把「推 l4-sync ＋ 校验远端一致」封成一条 ✓）★
#   **∴ 为什么 ✗**：**`cmd_sync` 有**一致守卫**（**∴ 不一致就拒 ✓）
#     ＋ **∴ 而**忘了推就会**白等一轮** ✓ ⇒ **∴ 所以**：**先 push 再校验** ✓ ★
cmd_push() {
  local br; br="$(git branch --show-current)"
  echo "  当前分支：${br}（**同步分支 ${L4_BRANCH} ✓）"
  git push -q origin "HEAD:${L4_BRANCH}" 2>&1 | tail -2
  local head remote
  head="$(git rev-parse --short=7 HEAD)"
  remote="$(git ls-remote origin -h "refs/heads/${L4_BRANCH}" 2>/dev/null | cut -c1-7)"
  if [ "${head}" = "${remote}" ]; then echo "  ✓ 已同步（${head} ✓）"; else echo "  ★ 不一致：本地 ${head}｜远端 ${remote}"; fi
  [ "${br}" = "${L4_BRANCH}" ] || echo "  （**注意**：当前不在 ${L4_BRANCH} ⇒ 推的是 HEAD ✓）"
}

cmd_sync() {
  # **∴ 走 GitHub ✗** ⇒ **∴ 不依赖 scp／rsync**，且**用已推送的提交** ✓
  local head remote_head
  # **★ 两侧必须用**同一个长度** ✗ ★**（**第 495 轮实测 ✓）：
  #   **∴ 原来的 bug ✗**：**左边 `git rev-parse --short` 给**8**字符（`69286a00` ✓）**✗
  #     ＋ **∴ 右边 `cut -c1-7` 给**7**字符（`69286a0` ✓）** ✓
  #       ⇒ **∴ 于是**：**永远判「不一致 ⇒ 先 push」** ✓
  #         ＋ **∴ 而**代码**明明已经推送了** ✓ ⇒ **∴ 那**是**假警报** ✓ ★**** ✓✓
  #   **∴ 现在 ✗**：**两侧都取 7 字符** ✓（**∴ 与 GitHub 的短哈希一致 ✓）★**** ✓✓
  head="$(git rev-parse --short=7 HEAD)"
  remote_head="$(git ls-remote origin -h refs/heads/${L4_BRANCH} 2>/dev/null | cut -c1-7)"
  [ "${head}" = "${remote_head}" ] || die "本地 HEAD（${head}）与远端 ${L4_BRANCH}（${remote_head}）不一致 ⇒ 先 push（**不许**测一份未推送的代码 ✓）" 2
  l4_ssh "cd ${L4_DIR} && git fetch -q origin && git checkout -q ${L4_BRANCH} 2>/dev/null || true; git reset -q --hard origin/${L4_BRANCH} && git log --oneline -1 && echo '分支：'\$(git branch --show-current)'｜工作区：'\$(git status --porcelain | wc -l)' 项'" 2>&1 | sed 's/^/  /'
}

cmd_accounts() {
  local rounds="${1:-3}" width="${2:-3840}" height="${3:-2160}"
  # **∴ 先确保构建好 ✓**
  l4_ssh "cd ${L4_DIR} && ls target/release/yanshi-serve >/dev/null 2>&1 || { export PATH=\$HOME/.cargo/bin:\$PATH; cargo build --release -p yanshi-http --bin yanshi-serve --features gpu; }" >/dev/null 2>&1
  # **∴ 用仓库自带的工具 ✓**（**它读 `/proc`，Linux 上正好 ✓）
  l4_ssh "cd ${L4_DIR} && pkill -9 -x yanshi-serve 2>/dev/null; sleep 1; node --version >/dev/null 2>&1 || { echo '  ✗ L4 上没有 node ⇒ 装：apt-get install -y nodejs npm'; exit 1; }; timeout 1500 node scripts/tool-render-cost-accounts.mjs --spawn --rounds ${rounds} --baseline target/release/yanshi-serve --compare 'CPU=--gpu off' --compare 'GPU=--gpu on' 2>&1 | tail -18" 2>&1 | sed 's/^/  /'
}

# **★ 远端作业账（**机器可读 ✓）✗ ★**（**第 494 轮 ✓；**用户问「tmux 是不是更方便」换来 ✓）
#
# **∴ 为什么不用 tmux 收账 ✗**（**两边都写 ✓）**：
#   **∴ tmux 的**收益**：**一个 ssh 连接里**并行多窗口**✗（**∴ 绕开 colab 的
#     「**一次只允许一个连接**（HTTP 429 ✓）」限制 ✓）＋ **∴ 交互式看进度 ✓
#   **∴ tmux 的**代价（**决定性的 ✓）**：**`tmux capture-pane` 给的是**屏幕文本**✗
#     ⇒ **∴ 拿不到**退出码**✗ ⇒ **∴ 判据里**红／绿**就丢了** ✓
#       ＋ **∴ 且** ANSI 转义与折行会**污染输出** ✓
#         ⇒ **★ 所以 ✗ ★**：**「人肉看」用 tmux**✗ 、**「机器收账」用
#           **`log` ＋ `rc` 文件**** ✓
#           ⇒ **∴ 本组子命令就是后者 ✓：
#             **`bg <名字> <命令>`**（**起作业 ＋ 写 log／rc／pid ✓）
#             ＋ **`jobs`**（**列作业 ＋ 各自退出码 ✓）
#             ＋ **`tail <名字> [行数]`** ＋ **`rc <名字>`** ✓ ★**** ✓✓
L4_JOBS="${L4_JOBS:-/root/l4jobs}"

# **★ 起一个**脱离连接**的作业 ✗ ★**
#
# **∴ 三样都写 ✗**：stdout／stderr ⇒ `.log`｜**退出码 ⇒ `.rc`**｜pid ⇒ `.pid` ✓
#   ⇒ **∴ 于是**：**下一轮回来能**判红绿****✗ ，**不靠屏幕文本** ✓
#
# **★ 必须**外层再包一层**才能拿到退出码 ✗ ★**（**第 494 轮实测 ✓）：
#   **∴ 原来的写法 ✗**：`bash -c "<命令>; echo $? > rc"`
#     ⇒ **∴ 若**命令自己 `exit N`**✗ ⇒ **∴ 内层 shell**立刻退出** ✓
#       ⇒ **∴ 于是**：**那句 `echo` **永远不执行**✗ ⇒ **∴ `.rc` **不存在** ✓
#         ＋ **∴ 而** `jobs` **只会显示「运行中」**✗
#           ⇒ **★ 判据永远拿不到红绿 ★** ✓
#             （**∴ 实测**：一个 `… ; exit 7` 的作业**打印完了 `done`，rc 仍是「还在跑」** ✓）★**** ✓✓
#   **∴ 现在 ✗**：**内层随便 `exit`** ＋ **外层捕获它的退出码** ✓
#     ＋ **∴ 且**外层命令用 `printf %q` **引用**内层命令**✗
#       ⇒ **∴ 于是**：**内层里的引号／分号都**不会被外层吃掉** ✓ ★**** ✓✓
cmd_bg() {
  local name="${1:-}" cmd="${2:-}"
  [ -n "${name}" ] && [ -n "${cmd}" ] || die "用法：scripts/l4.sh bg <名字> <命令>" 2
  # **∴ 命令串里**只放命令**✗ ⇒ **∴ 注释一律写在引号外** ✓
  #   （**∴ 教训**：**上一次我把注释塞进双引号里 ⇒ 里面的引号把整串**拆坏了** ✓）
  local quoted
  quoted="$(printf '%q' "${cmd}")"
  l4_ssh_retry "mkdir -p ${L4_JOBS}; cd ${L4_JOBS}
rm -f ${name}.log ${name}.rc ${name}.pid
setsid nohup bash -c 'bash -c ${quoted}; echo \$? > ${L4_JOBS}/${name}.rc' > ${L4_JOBS}/${name}.log 2>&1 < /dev/null &
echo \$! > ${L4_JOBS}/${name}.pid
echo started-${name}" 2>&1 | sed 's/^/  /'
}

# **★ 列作业 ＋ 退出码 ✗ ★**
#
# **∴ 写法教训（**第 495 轮实测 ✓）✗**：**∴ 第一版把远端脚本写在**双引号**里**✗
#   ＋ **∴ 到处用 `\\$`／`\\"` 转义** ✗
#     ⇒ **∴ 于是**：**远端拿到的字符串**与预期不符** ✓
#       ⇒ **∴ 症状 ✗**：**一个已经写完 `.rc` 的作业，`jobs` 仍显示「运行中」** ✓
#         （**∴ 而**同一条 rc 用 `rc <名字>` **读得到** ✓）★**** ✓✓
#   **∴ 现在 ✗**：**远端脚本用**单引号**包起来**✗（**∴ 本地不展开 `$` ✓）
#     ⇒ **∴ 与能正常工作的 `cmd_status` 同风格 ✓ ★**** ✓✓
cmd_jobs() {
  l4_ssh_retry 'cd '"${L4_JOBS}"' 2>/dev/null || { echo "  （还没有作业目录）"; exit 0; }
printf "  %-14s %-8s %-8s %s\n" 名字 状态 退出码 日志行数
for f in *.log; do
  [ -e "$f" ] || continue
  n="${f%.log}"
  if [ -f "$n.rc" ]; then
    st=已完成; rc="$(cat "$n.rc")"
  else
    st=运行中; rc=—
  fi
  printf "  %-14s %-8s %-8s %s\n" "$n" "$st" "$rc" "$(wc -l < "$f")"
done' 2>&1 | sed 's/^/  /'
}

cmd_tail() {
  local name="${1:-}" n="${2:-20}"
  [ -n "${name}" ] || die "用法：scripts/l4.sh tail <名字> [行数]" 2
  l4_ssh_retry "tail -n ${n} ${L4_JOBS}/${name}.log 2>/dev/null || echo '  （没有这个作业的日志）'" 2>&1 | sed 's/^/  /'
}

cmd_rc() {
  local name="${1:-}"
  [ -n "${name}" ] || die "用法：scripts/l4.sh rc <名字>" 2
  l4_ssh_retry "if [ -f ${L4_JOBS}/${name}.rc ]; then echo \"  ${name} 退出码 = \$(cat ${L4_JOBS}/${name}.rc)（**0 才是绿 ✓）\"; else echo '  ${name} 还在跑（没有 rc 文件 ✓）'; fi" 2>&1 | sed 's/^/  /'
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
  help|-h|--help)
    # **★ 从**本文件自己的 case 分发**生成用法 ✗ ★**（**第 609 轮 ✓；**用户要求「简化操作」 ✓）
    #   **∴ 为什么从文件生成 ✗**：**手写列表**会过时**✗
    #     ＋ **∴ 而**分发是**唯一真相**（**∴ 谁**加了分支 ⇒ **∴ 立刻**出现在 help 里 ✓）★
    echo "  用法：scripts/l4.sh <子命令> [参数…]"
    echo ""
    echo "  子命令（**从本脚本的分发自动生成 ✓）："
    grep -oE '^  [a-z|-]+\).*' "$0" | sed 's/) */)  /' | grep -v '^  help' | sed 's/^/   /'
    echo ""
    echo "  环境变量：L4_SESSION（缺省 dawang）｜L4_DIR（缺省 /content/yanshi）｜L4_BRANCH（缺省 l4-sync）"
    exit 0 ;;
  sync)        shift; cmd_sync "$@" ;;
  up-state) cmd_up_state "$@" ;;
  ping)        shift; cmd_ping "$@" ;;
  gate)        shift; cmd_gate "$@" ;;
  clean)       shift; cmd_clean "$@" ;;
  push)        shift; cmd_push "$@" ;;
  tnew)        shift; cmd_tnew "$@" ;;
  tsend)       shift; cmd_tsend "$@" ;;
  twin)        shift; cmd_twin "$@" ;;
  tlog)        shift; cmd_tlog "$@" ;;
  tls)         shift; cmd_tls "$@" ;;
  tkill)       shift; cmd_tkill "$@" ;;
  accounts)    shift; cmd_accounts "$@" ;;
  # **★ 平衡点扫描（**目标第 3 条 ✓）✗ ★**（**第 758 轮 ✓）
  #   **∴ 它**扫一组像素数**✗ ⇒ **∴ 找**GPU 与 CPU 的交点** ✓
  #     ＋ **∴ 并**强制**打印适配器名**✗（**∴ 第 318 轮的坑 ✓）
  #       ＋ **∴ 退出码 ✗**：**0 找到｜1 没找到｜2 前置错｜3 本机无适配器（**不可判 ✓）★
  break-even|breakeven|be)
              shift
              l4_ssh "cd ${L4_DIR} && node scripts/tool-break-even.mjs $*" ;;
  # **∴ 作业账（**机器可读 ✓）**：**bg／jobs／tail／rc** ✓
  bg)          shift; cmd_bg "$@" ;;
  jobs)        shift; cmd_jobs "$@" ;;
  tail)        shift; cmd_tail "$@" ;;
  rc)          shift; cmd_rc "$@" ;;
  down)        shift; cmd_down "$@" ;;
  sh|shell)    shift; cmd_sh "$@" ;;
  ""|-h|--help) sed -n '2,40p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//' ;;
  *) die "未知子命令：${1}（用 --help 看用法）" 2 ;;
esac
