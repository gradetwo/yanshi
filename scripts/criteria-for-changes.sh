#!/usr/bin/env bash
# **改动文件 → 受影响的判据子集** ✓（第 1293 轮；按外部 2026-10-20 的节奏建议）
#
# **⇒ 它解决的问题** ✗：
#   本机跑**全部**判据 = 20+ 分钟 + release 构建；而 CI 一轮 20–40 分钟。
#   ⇒ 于是「改一行 → 等一轮」把整块活切碎（外部实测：色彩管线 Phase 1 全天只落 1 个 commit）。
#   ⇒ 而**绝大多数改动只影响少数判据** ⇒ 所以本机只需要跑那个子集。
#
# **⇒ 用法**（默认查**未提交**的改动 + 与 origin/main 的差）：
#   bash scripts/criteria-for-changes.sh              # 列出受影响的判据
#   bash scripts/criteria-for-changes.sh --run        # 直接跑它们（经 run-criteria-local.sh）
#   bash scripts/criteria-for-changes.sh --since HEAD~5
#
# **⇒ 映射规则**（保守：只做**可能**受影响的猜测，宁可多跑）：
#   · 改 `crates/yanshi-render/**`   ⇒ **全部** tool-*/browser-* 判据（渲染影响一切像素）
#   · 改 `crates/yanshi-server/**`   ⇒ tool-*/serve-*/mcp-*（工具与 HTTP 面）
#   · 改 `web/**` 或 `assets/**`     ⇒ browser-*/tool-pwa-*
#   · 改 `scripts/**`                ⇒ **该脚本自己**（若它是判据）+ check-*
#   · 改 `docs/**`                   ⇒ check-*（文档类判据）+ tool-*-doc
#   · 改 `crates/yanshi-core/**`     ⇒ tool-*（状态与折叠层）
#   · 其它（CI 配置等）              ⇒ **不列**（本机无需跑）
#
# **⇒ 退出码**：0 正常（哪怕子集为空）；3 不可判（不在 git 仓库里等）
set -u

SINCE=""
RUN=0
while [ $# -gt 0 ]; do
  case "$1" in
    --run) RUN=1 ;;
    --since) shift; SINCE="${1:-}" ;;
    *) echo "  用法: $0 [--run] [--since <rev>]" >&2; exit 3 ;;
  esac
  shift
done

if ! git rev-parse --git-dir > /dev/null 2>&1; then
  echo "  ⊘ 不可判：不在 git 仓库里" >&2
  exit 3
fi

# 改动清单：未提交 + 与上游的差
if [ -n "$SINCE" ]; then
  CHANGED=$(git diff --name-only "$SINCE" 2>/dev/null)
else
  CHANGED=$( { git diff --name-only; git diff --cached --name-only; } 2>/dev/null | sort -u )
  # 若没有未提交改动 ⇒ 退回到「与 origin/main 的差」
  if [ -z "$CHANGED" ]; then
    CHANGED=$(git diff --name-only origin/main...HEAD 2>/dev/null)
  fi
fi

if [ -z "$CHANGED" ]; then
  echo "  （没有改动 ⇒ 无需跑判据）"
  exit 0
fi

echo "  改动文件数：$(printf '%s\n' "$CHANGED" | wc -l)"

PICK=""
add_all() {
  # $1 = glob
  for f in scripts/$1; do
    [ -f "$f" ] || continue
    case "$(basename "$f")" in
      criteria-for-changes.sh|run-criteria-local.sh|run-criteria.sh|survey-all-criteria.sh|watch-ci.sh) continue ;;
    esac
    PICK="$PICK $f"
  done
}

has() { printf '%s\n' "$CHANGED" | grep -qE "$1"; }

# **★ 按文件名细化** ✓（第 1293 轮）：`server/**` 一刀切会给出 248 个（≈ 全量）⇒ 那等于没分。
#   ⇒ 规则改成「**具体文件 → 对应族的判据**」⇒ 子集才真的小。
#
#   族名与判据前缀的对应（本仓既有命名）：
#     tools_brush.rs    ⇒ tool-brush-*, tool-stroke-*, tool-taper-*
#     tools_smart.rs    ⇒ tool-smart-*
#     tools_selection.rs⇒ tool-selection-*
#     tools_mask.rs     ⇒ tool-mask-*
#     tools_bitmap.rs   ⇒ tool-import-*, tool-put-blob-*, tool-export-*
#     tools_io.rs       ⇒ tool-export-*
#     tools_psd.rs      ⇒ tool-psd-*
#     tools_effect.rs   ⇒ tool-effect-*, tool-filter-*
#     tools_read.rs     ⇒ tool-read-*, tool-sample-*
#     tools_registry.rs ⇒ tool-surface-*, tool-examples-*, check-tool-*
#     tools.rs（其余）  ⇒ tool-*（保持保守）
for f in $CHANGED; do
  case "$f" in
    crates/yanshi-server/src/tools_brush.rs)     add_all 'tool-brush-*.mjs'; add_all 'tool-stroke-*.mjs'; add_all 'tool-taper-*.mjs' ;;
    crates/yanshi-server/src/tools_smart.rs)     add_all 'tool-smart-*.mjs' ;;
    crates/yanshi-server/src/tools_selection.rs) add_all 'tool-selection-*.mjs' ;;
    crates/yanshi-server/src/tools_mask.rs)      add_all 'tool-mask-*.mjs' ;;
    crates/yanshi-server/src/tools_bitmap.rs)    add_all 'tool-import-*.mjs'; add_all 'tool-put-blob-*.mjs'; add_all 'tool-export-*.mjs' ;;
    crates/yanshi-server/src/tools_io.rs)        add_all 'tool-export-*.mjs' ;;
    # **新模块 `tools_bgfield.rs`** ✓（第 1461 轮补）：否则改它时**本地定向跑会漏掉**它自己的判据 ✗
    crates/yanshi-server/src/tools_bgfield.rs)   add_all 'tool-fit-background-field.mjs' ;;
    crates/yanshi-server/src/tools_psd.rs)       add_all 'tool-psd-*.mjs' ;;
    crates/yanshi-server/src/tools_effect.rs)    add_all 'tool-effect-*.mjs'; add_all 'tool-filter-*.mjs' ;;
    crates/yanshi-server/src/tools_read.rs)      add_all 'tool-read-*.mjs'; add_all 'tool-sample-*.mjs' ;;
    crates/yanshi-server/src/tools_registry.rs)  add_all 'tool-surface-*.mjs'; add_all 'tool-examples-*.mjs'; add_all 'check-*.mjs' ;;
    crates/yanshi-server/src/tools.rs)           add_all 'tool-*.mjs'; add_all 'serve-*.mjs'; add_all 'mcp-*.mjs' ;;
    crates/yanshi-server/src/*)                  add_all 'serve-*.mjs'; add_all 'mcp-*.mjs' ;;
    crates/yanshi-render/src/color.rs)           add_all 'tool-color-*.mjs'; add_all 'tool-*-color-*.mjs'; add_all 'tool-vermeer-*.mjs' ;;
    # **★ `brush.rs` 是**落笔实现**（含净室揉合）⇒ 只影响笔刷族，不是全部** ✓（第 1313 轮实测：
    #   原来落到 `render/*` ⇒ **288 个（≈ 全量）** ⇒ 等于没分）
    crates/yanshi-render/src/brush.rs)           add_all 'tool-brush-*.mjs'; add_all 'tool-stroke-*.mjs'; add_all 'tool-taper-*.mjs'; add_all 'tool-bristle-*.mjs'; add_all 'tool-visible-delta.mjs' ;;
    crates/yanshi-render/src/buffer.rs)          add_all 'tool-*.mjs' ;;
    crates/yanshi-render/src/tile.rs)            add_all 'tool-*.mjs' ;;
    crates/yanshi-render/src/*)                  add_all 'tool-*.mjs'; add_all 'browser-*.mjs' ;;
    crates/yanshi-core/src/*)                    add_all 'tool-*.mjs' ;;
    web/*|assets/*)                              add_all 'browser-*.mjs'; add_all 'tool-pwa-*.mjs' ;;
    docs/*)                                      add_all 'check-*.mjs'; add_all 'tool-*-doc.mjs' ;;
    # **★ `vendor/hokusai-core` 是**笔刷引擎本身** ⇒ 改它影响**所有**笔刷判据** ✓（第 1299 轮实测漏过）
    #   **当时的错**：改了 `stroke.rs`/`color.rs` ⇒ 本工具报「0 个」⇒ 而那是**最重要的一类改动** ✗
    vendor/hokusai-core/src/*)                   add_all 'tool-brush-*.mjs'; add_all 'tool-stroke-*.mjs'; add_all 'tool-taper-*.mjs'; add_all 'tool-color-*.mjs'; add_all 'check-vendor-determinism-ratchet.mjs' ;;
    vendor/*)                                    add_all 'tool-*.mjs'; add_all 'check-vendor-determinism-ratchet.mjs' ;;
    # **⇒ `service.rs` 管工程包的读写** ✓（第 1352 轮补）：包格式判据必须跟着跑
    #   **⇒ 为什么之前漏了** ✗：`service.rs` 原来**没有任何规则** ⇒ 改它时报「0 个」✗
    crates/yanshi-server/src/service.rs)         add_all 'tool-import-*.mjs'; add_all 'tool-export-*.mjs'; add_all 'check-project-package-blob-naming.mjs' ;;
  esac
done

# 改判据脚本本身 ⇒ 那个脚本必须跑
for f in $CHANGED; do
  case "$f" in
    scripts/tool-*.mjs|scripts/mcp-*.mjs|scripts/check-*.mjs|scripts/serve-*.mjs|scripts/browser-*.mjs)
      case "$(basename "$f")" in
        criteria-for-changes.sh|run-criteria-local.sh|run-criteria.sh|survey-all-criteria.sh|watch-ci.sh) ;;
        *) PICK="$PICK $f" ;;
      esac
      ;;
  esac
done

# 去重排序
PICK=$(printf '%s\n' $PICK | sort -u | tr '\n' ' ')

if [ -z "${PICK// /}" ]; then
  echo "  ⇒ 受影响的判据：**0 个**（这类改动本机无需跑判据，直接推 CI）"
  exit 0
fi

N=$(printf '%s\n' $PICK | wc -w)
echo "  ⇒ 受影响的判据：**$N 个**"
printf '%s\n' $PICK | tr ' ' '\n' | sed 's/^/     /'

if [ "$RUN" -eq 1 ]; then
  echo "  --- 开始跑（每个都是 exit 0/1/3；3 = 不可判，不算失败）---"
  FAIL=0
  for c in $PICK; do
    bash scripts/run-criteria-local.sh "$c" > /tmp/crit-$(basename "$c" .mjs).log 2>&1
    RC=$?
    case "$RC" in
      0) echo "  ✓ $(basename "$c")" ;;
      3) echo "  ⊘ $(basename "$c")（不可判）" ;;
      *) echo "  ✗ $(basename "$c") ⇒ 日志 /tmp/crit-$(basename "$c" .mjs).log"; FAIL=$((FAIL+1)) ;;
    esac
  done
  echo "  ⇒ 失败 $FAIL 个（⊘ 不算失败）"
  [ "$FAIL" -eq 0 ] || exit 1
fi
