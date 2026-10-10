#!/usr/bin/env bash
# **★ 本地一键部署 PWA ✗ ★**（**第 492 轮 ✓；**用户要求「常用操作做成脚本 ✓」）
#
# **∴ 为什么需要它 ✗**：**线上 PWA 只有在**① 打 `v*` tag**✗ 或 **② 手动跑 workflow** 时才部署** ✓
#   ⇒ **∴ 而**手动跑 workflow 要**等 CI 的排队与构建**✗
#     ＋ **∴ 且**它**依赖仓库 secret `CLOUDFLARE_API_TOKEN`** ✓
#       ⇒ **∴ 于是**：**本地用 `.env` 里的 token 直接部署**✗
#         ⇒ **∴ 迭代**更快**✗ ＋ **∴ 不依赖**仓库 secret** ✓ ★**** ✓✓
#
# **∴ 它照 CI 的步骤做同一件事 ✗**（**`pwa.yml` ✓，**所以两条路不会漂移** ✓）：
#   ① 构建 wasm 内核 ⇒ ② 装 wasm-bindgen（**版本取自 `Cargo.lock` ✓）
#   ⇒ ③ 生成绑定 `crates/yanshi-wasm/pkg/` ⇒ ④ 同步 viewer ＋ 导出静态页
#   ⇒ ⑤ 跑 PWA 判据（**资源完整性／缓存戳幂等／本机可跑的 ✓）
#   ⇒ ⑥ `wrangler deploy`
#
# **∴ 用法 ✗**：
#   scripts/deploy-pwa.sh              # 构建 ＋ 判据 ＋ 真部署
#   scripts/deploy-pwa.sh --dry-run    # 只构建 ＋ 判据 ＋ `wrangler deploy --dry-run`（**不发布 ✓）
#   scripts/deploy-pwa.sh --skip-build # 产物已在，只判据 ＋ 部署（**快 ✓）
#   scripts/deploy-pwa.sh --check      # 只跑判据，**不部署** ✓
#
# **∴ 凭据 ✗**：从 `.env` 读 `CLOUDFLARE_API_TOKEN`（**也可以从环境变量给 ✓）。
#   **∴ 绝不回显 token** ✓（**只打印它的长度 ✓）
#
# **∴ 两面（**AGENTS.md 第 3 条 ✓）★**：
#   **∴ 收益 ✗**：**本地一次命令上线**✗ ＋ **不占用 CI 队列** ＋ **不依赖仓库 secret** ✓**** ✓✓
#   **∴ 代价 ✗**：**部署用了**本机的工具链**✗ ⇒ **∴ 若**本机与 CI 的工具链版本不同**✗
#     ⇒ **∴ 产物**可能**与 CI 的**不完全一致** ✓
#       ＋ **∴ 所以**：**发布给用户的版本**建议仍走 CI（**可复现 ✓）
#         ＋ **∴ 而**本脚本**适合**快速验证／紧急修复** ✓ ★**** ✓✓
#     ＋ **∴ 且**：**本机需要 `wasm32-unknown-unknown` 目标**✗
#       ⇒ **∴ 没有就**响亮失败并给命令** ✓ ★**** ✓✓
#
# **∴ 退出码 ✗**：0 成功｜1 某一步失败｜2 用法／前提问题

set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROOT="$(pwd)"

DRY_RUN=0
SKIP_BUILD=0
CHECK_ONLY=0
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run)    DRY_RUN=1; shift ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --check)      CHECK_ONLY=1; shift ;;
    -h|--help)    sed -n '2,40p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) printf '未知参数：%s\n' "$1" >&2; exit 2 ;;
  esac
done

say() { printf '\n== %s ==\n' "$*"; }

# **∴ 凭据：**绝不回显** ✗**
CF_TOKEN="${CLOUDFLARE_API_TOKEN:-}"
if [ -z "${CF_TOKEN}" ] && [ -f "${ROOT}/.env" ]; then
  CF_TOKEN="$(grep -a '^CLOUDFLARE_API_TOKEN=' "${ROOT}/.env" | head -1 | cut -d= -f2- | tr -d '"'"'"' \r')"
fi
if [ -n "${CF_TOKEN}" ]; then
  say "凭据：CLOUDFLARE_API_TOKEN 已就绪（长度 ${#CF_TOKEN} ✓ —— **不打印内容** ✓）"
else
  say "凭据：**没有** CLOUDFLARE_API_TOKEN ⇒ 只能 --dry-run／--check"
fi

# **∴ 步骤 ①：wasm 目标必须在 ✗**（**否则后面必炸，而且报错很绕 ✓）
if [ "${SKIP_BUILD}" = 0 ]; then
  say "① 检查 wasm32 目标"
  if ! rustup target list --installed 2>/dev/null | grep -q '^wasm32-unknown-unknown$'; then
    echo "  ✗ 本机没有 wasm32-unknown-unknown 目标" >&2
    echo "     ⇒ 装它：rustup target add wasm32-unknown-unknown" >&2
    exit 2
  fi
  echo "  ✓ 已装"

  say "② 构建 wasm 内核"
  cargo build -p yanshi-wasm --release --target wasm32-unknown-unknown

  say "③ 装 wasm-bindgen CLI（**版本来自 Cargo.lock ✓）"
  VER="$(scripts/wasm-bindgen-version.sh)"
  echo "  wasm-bindgen（Cargo.lock）= ${VER}"
  if ! command -v wasm-bindgen >/dev/null 2>&1 || [ "$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')" != "${VER}" ]; then
    cargo install wasm-bindgen-cli --version "${VER}" --locked
  else
    echo "  ✓ 已是 ${VER}"
  fi

  say "④ 生成绑定（pkg/）"
  mkdir -p crates/yanshi-wasm/pkg
  wasm-bindgen --target web --out-dir crates/yanshi-wasm/pkg \
    target/wasm32-unknown-unknown/release/yanshi_wasm.wasm
else
  say "①–④ 已跳过（--skip-build ✓）"
fi

say "⑤ 同步 viewer ＋ 静态页 ＋ 内核（**与 CI 同一套脚本 ✓）"
node scripts/pwa-sync-viewer.mjs
node scripts/pwa-sync-wasm.mjs

say "⑥ 判据（**失败即停 ✓）"
# **∴ 这些是**本机可跑**的 PWA 判据 ✗**（**CI 里还跑浏览器判据 ✓）
node scripts/tool-pwa-assets.mjs
node scripts/tool-sw-stamp-idempotent.mjs
# **∴ 本机可跑的**通用判据**也带上 ✓**
node scripts/tool-workflow-wasm-bindgen-pin.mjs

if [ "${CHECK_ONLY}" = 1 ]; then
  say "⇒ --check：判据全绿 ⇒ **不部署** ✓"
  exit 0
fi

say "⑦ 部署（**wrangler ✓）"
[ -n "${CF_TOKEN}" ] || { echo "  ✗ 没有 token ⇒ 用 --dry-run／--check，或配 .env" >&2; exit 2; }
export CLOUDFLARE_API_TOKEN="${CF_TOKEN}"
if [ -n "${CLOUDFLARE_ACCOUNT_ID:-}" ]; then
  export CLOUDFLARE_ACCOUNT_ID
fi
if [ "${DRY_RUN}" = 1 ]; then
  npx --yes wrangler deploy --dry-run --outdir /tmp/pwa-dist
  say "⇒ dry run 通过（**未发布** ✓）"
else
  npx --yes wrangler deploy
  say "⇒ 已部署 ⇒ 自检线上是否真的更新："
  # **∴ 自检（**不许只说「成功了」 ✓）★**
  online="$(curl -s --max-time 30 https://yanshi-online.wangda.today/viewer-app.js | grep -c 'putBlob(db, hash' || true)"
  echo "  线上 viewer-app.js 里的修复标记 = ${online} 处（**每次上线的内容不同，这里只证明文件可读 ✓）"
  echo "  ⇒ 请按**本次改动的特征串**核对；或看 CI 的 step summary ✓"
fi
