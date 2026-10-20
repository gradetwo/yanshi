#!/usr/bin/env bash
# 本机跑判据的**唯一入口** —— 先确认二进制比源码新，再起服务跑判据。
#
# **为什么需要这个脚本**（第 973 轮）：第 947-956 轮我用**陈旧的**
# `target/release/yanshi-serve` 跑判据，测出「[200,…] 与 [100,…] 都给 255」的假现象，
# 然后花 10 轮去追一个**不存在的 bug**，期间对同一处代码给出过**三个互相矛盾的结论**。
#
# ⇒ 「本机跑判据前先重建」这条规矩写在 AGENTS.md 里，而**我违反了它**。
# ⇒ 所以把它变成**不可绕过的一步**（971 轮的观察：能被脚本强制的才会生效）。
#
# 用法：
#   bash scripts/run-criteria-local.sh <判据脚本> [判据参数...]
#   bash scripts/run-criteria-local.sh scripts/tool-color-array-domain.mjs
#
# 退出码：判据自己的退出码（0 通过 / 1 产品失败 / 3 不可判）

set -uo pipefail
cd "$(dirname "$0")/.."

CRIT="${1:?用法: $0 <判据脚本> [参数...]}"
shift

BIN=target/release/yanshi-serve
# **★ 必须同时扫 `vendor/`** ✓（第 1308 轮；外部 2026-10-20 的报告指出）
#   **漏项的代价**：Phase 1 改的是 `vendor/hokusai-core/src/stroke.rs` ⇒ 而这里只扫
#     `crates/*/src` ⇒ 于是**二进制陈旧却判「新」** ⇒ 判据测的是**旧代码** ✗
#   ⇒ 而那正是 `N3E-01` 的同一类错（静默用陈旧二进制）⇒ 所以扫描范围要**覆盖所有 .rs**
SRC_MAX=$(find crates vendor -name '*.rs' -newer "$BIN" 2>/dev/null | head -5)

if [ ! -x "$BIN" ] || [ -n "$SRC_MAX" ]; then
  echo "  ⟳ 二进制比源码旧（或不存在）⇒ 先重建"
  [ -n "$SRC_MAX" ] && echo "$SRC_MAX" | sed 's/^/      新于二进制：/'
  # **★ 必须看 `cargo build` 的退出码，不能只看二进制是否存在** ✓（第 1287 轮；N3E-01）
  #   **缺陷**：编译失败时**旧二进制还在** ⇒ `[ -x "$BIN" ]` 为真 ⇒ 脚本**继续用陈旧二进制跑判据**
  #     ⇒ 于是判据可能**假绿**（测的是旧代码）✗ —— 而这恰好发生在「源码比二进制新」这个**主场景**
  #   **修法**：把 build 的退出码取出来 ⇒ 非 0 就 exit 3（不可判，不是产品失败）
  BUILD_LOG=$(mktemp)
  cargo build --release -p yanshi-http > "$BUILD_LOG" 2>&1
  BUILD_RC=$?
  tail -2 "$BUILD_LOG" | sed 's/^/      /'
  rm -f "$BUILD_LOG"
  if [ "$BUILD_RC" -ne 0 ]; then
    echo "  ⊘ 不可判：重建失败（cargo build 退出码 ${BUILD_RC}）⇒ 拒绝用陈旧二进制跑判据" >&2
    exit 3
  fi
  if [ ! -x "$BIN" ]; then
    echo "  ⊘ 不可判：重建成功但没有二进制（$BIN 不存在）" >&2
    exit 3
  fi
else
  echo "  ✓ 二进制比源码新"
fi

# 起一个临时服务（判据自己建文档）
ROOT=$(mktemp -d "${TMPDIR:-/tmp}/yanshi-crit-XXXXXX")
PORT=$(( 23000 + RANDOM % 2000 ))
# **两类判据都适用**（第 990 轮实测）：
#   ① 自建文档的（164 个）⇒ 忽略 --doc，自己调 /api/documents
#   ② 自建**服务**的（tool-sample-color.mjs / tool-paint-memory.mjs）
#      ⇒ spawn 自己的服务（随机端口 158xx）⇒ 与这里起的**不冲突**
#      ⇒ 实测 tool-sample-color.mjs 经本脚本跑：退出码 0 ✓
#
# **987 轮那次把这一行的括号写坏了**（`/api/documents` 后面跟了两个 U+FFFD 替换字符）
# ⇒ 一并修正（不影响执行，因为它在注释里，但文件是坏的）
# 所以这里的 --doc/--width/--height 只是**服务的初始文档**，判据不会用它。
# ⇒ 不要以为判据跑在 400x400 上（第 987 轮实测：0 个判据用 doc=g）。
# **服务端也要设 `YANSHI_EXPORT_DIR`**（第 1088 轮查清）：
#   `guarded_output_path`（`tools.rs:23184-23187`）里
#     `YANSHI_EXPORT_DIR` 未设 ⇒ 缺省是 `"exports"` ⇒ **相对服务端进程的 CWD** ✗
#     ⇒ 而 CWD 是仓库根 ⇒ 导出文件落到 `<仓库>/exports/`，**不在 `--root` 下** ✗
#     ⇒ 判据按 `YANSHI_WORKSPACE/exports` 找 ⇒ **找不到** ⇒ 抛错 ⇒ exit 3 ✗
#   ⇒ 所以**启动服务时**就要设（只在 node 前 export 是没用的：那不影响已在跑的服务）✓
YANSHI_EXPORT_DIR="$ROOT/exports" \
  "$BIN" --root "$ROOT" --doc _bootstrap --width 64 --height 64 --bind "127.0.0.1:$PORT" > "$ROOT/s.log" 2>&1 &
PID=$!
trap 'kill "$PID" 2>/dev/null; rm -rf "$ROOT"' EXIT

for _ in $(seq 1 60); do
  curl -s -m 1 -o /dev/null "http://127.0.0.1:$PORT/health" && break
  sleep 0.5
done

if ! curl -s -m 1 -o /dev/null "http://127.0.0.1:$PORT/health"; then
  echo "  ⊘ 不可判：服务没起来（判据前提不成立，不是产品问题）" >&2
  exit 3
fi

# **总是传 `<base> <doc> <token>`**（第 1076 轮）：有两类判据
#   ① 只吃 `<base>`（大多数）     ⇒ 多传两个参数会被**忽略** ✓（实测 node 不报错）
#   ② 吃 `<base> <doc> <token>`（`tool-engine-capabilities` / `tool-brush-settings` /
#      `tool-brush-params-doc`）⇒ 以前用本脚本跑会得 **exit 3**（不可判）✗
#   ⇒ 所以统一传三个，两类都能跑 ✓
BASE_URL="http://127.0.0.1:$PORT"
CRIT_TOKEN=$(curl -s -X POST "$BASE_URL/api/documents" \
  -H 'content-type: application/json' \
  -d '{"doc_id":"_bootstrap","width":64,"height":64}' \
  | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
if [ -z "$CRIT_TOKEN" ]; then
  echo "  ⊘ 不可判：拿不到 token（判据前提不成立，不是产品问题）" >&2
  exit 3
fi
# **补 CI 会设的环境变量**（第 1088 轮）：有些判据靠它们找导出文件
#   ⇒ `tool-cjk-text` 的候选目录是
#       [YANSHI_EXPORT_DIR, $YANSHI_WORKSPACE/exports, $YANSHI_WORKSPACE, exportDir, "."]
#   ⇒ **不设时只剩 `"."`** ⇒ 找不到 `<doc>.png` ⇒ 抛错 ⇒ exit 3（假「判据自身出错」）✗
#   ⇒ 而 CI 的 runner **会**导出 `YANSHI_WORKSPACE` ⇒ 所以 CI 上它能跑 ✓
#   ⇒ 所以本脚本也要设，否则**本机与 CI 不一致** ✗
export YANSHI_WORKSPACE="$ROOT"
export YANSHI_EXPORT_DIR="$ROOT"
# **只传 `<base>`**（第 1089 轮回退 1076 轮的决定）：
#   1076 轮我改成「总传 `<base> <doc> <token>`」⇒ 想让 3-参数判据也能跑 ✓
#   **∴ 而** 1089 轮量出来它**净效果为负** ✗：
#     · 受益：把 `argv[3]` 当 doc 的判据 = **3 个**
#     · 受害：把 `argv[3]` 当别的（`.yanshi` 路径等）= **5 个**
#       （实测 `tool-bitmap-decode-scope` 把 token 当 base ⇒ JSON 里显示
#        「Failed to parse URL from <token>/api/documents」⇒ exit 3）
#   ⇒ **∴ 所以**回退：只传 `<base>`，需要额外参数的**手动跑** ✓
#   ⇒ 判据数量：用 `argv[3]`/`argv[4]` 的共 **31 个**（`grep -lE 'argv\[3\]'`）
# **★ 按需传** ✓（第 1267 轮）：判据源码里出现 `argv[3]`/`argv[4]` ⇒ 它要 `<doc> <token>`
#   ⇒ 传三个；否则只传 `<base>`（避免第 1076 轮那次「doc 被当 base」⇒
#   `Failed to parse URL from <token>/api/documents` ✗）
# **★ 三种取参写法都要认** ✓（第 1267 轮）：`argv[3]` / `argv[4]` / `process.argv.slice(2)`
#   ⇒ 我自己的新判据用的是 `slice(2)` ⇒ 前两种正则匹配不到它 ⇒ 于是又 exit 3 ✗
# **★ 判据进程也要拿到 `YANSHI_EXPORT_DIR`** ✓（第 1267 轮）
#   ⇒ 原来只有**服务进程**有（`:57` 的前缀赋值）⇒ 而判据要按它反推 `<root>`
#     （我的 `tool-import-blob-decodable` 就是这么找 CAS 的）⇒ 没有它 ⇒ 写不进 root ⇒ 假红 ✗
if grep -qE 'argv\[3\]|argv\[4\]|argv\.slice\(2\)' "$CRIT" 2>/dev/null; then
  YANSHI_EXPORT_DIR="$ROOT/exports" node "$CRIT" "$BASE_URL" _bootstrap "$CRIT_TOKEN" "$@"
else
  YANSHI_EXPORT_DIR="$ROOT/exports" node "$CRIT" "$BASE_URL" "$@"
fi
RC=$?
echo "  ⇒ 判据退出码=$RC"
exit $RC
