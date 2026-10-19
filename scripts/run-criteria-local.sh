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
SRC_MAX=$(find crates/*/src -name '*.rs' -newer "$BIN" 2>/dev/null | head -5)

if [ ! -x "$BIN" ] || [ -n "$SRC_MAX" ]; then
  echo "  ⟳ 二进制比源码旧（或不存在）⇒ 先重建"
  [ -n "$SRC_MAX" ] && echo "$SRC_MAX" | sed 's/^/      新于二进制：/'
  cargo build --release -p yanshi-http 2>&1 | tail -2 | sed 's/^/      /'
  if [ ! -x "$BIN" ]; then
    echo "  ⊘ 不可判：重建失败（判据前提不成立，不是产品问题）" >&2
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

node "$CRIT" "http://127.0.0.1:$PORT" "$@"
RC=$?
echo "  ⇒ 判据退出码=$RC"
exit $RC
