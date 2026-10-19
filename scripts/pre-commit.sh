#!/usr/bin/env bash
# **提交前的门**（第 834 轮）：我本场有 **3 次**在 clippy 红着的时候照样 commit
# （第 254 轮两次、第 833 轮两次），根因都是命令链里写了 `cargo clippy …; echo $?`
# 却**没让 commit 依赖它的结果**。
#
# ⇒ 用这个脚本代替「靠记得」：
#     bash scripts/pre-commit.sh && git commit -m "…"
#
# 它跑三件事，任一失败就 exit 1（于是 && 后面的 commit 不会执行）。
set -uo pipefail
fail=0

echo "  ① cargo fmt --all -- --check"
if ! cargo fmt --all -- --check > /tmp/precommit-fmt.log 2>&1; then
  echo "     ✗ fmt 不干净（跑 cargo fmt --all 再提交）"
  tail -5 /tmp/precommit-fmt.log
  fail=1
else
  echo "     ✓"
fi

echo "  ② cargo clippy --workspace --all-targets -- -D warnings"
if ! cargo clippy --workspace --all-targets -- -D warnings > /tmp/precommit-clippy.log 2>&1; then
  echo "     ✗ clippy 有错"
  grep -E "^(error|warning)" /tmp/precommit-clippy.log | head -6
  fail=1
else
  echo "     ✓"
fi

# ③ **磁盘卫生提醒**（第 1038 轮加）—— **只提醒，不阻塞** ✓
#   **∴ 为什么**：`AGENTS.md` 里有「build 后清 target/debug/{deps,build,incremental}」那条，
#     而本场我**读了但没做** ⇒ `target/debug` 涨到 **9.5 G**、磁盘 **63%** ✗
#     ⇒ 清完 **614 M**、磁盘 **60%** ✓（省 8.9 G）
#   ⇒ **∴ 而**它**没有脚本** ⇒ 没人执行（包括我）⇒ **∴ 所以**加在这里 ✓
DBG_KB=$(du -sk target/debug 2>/dev/null | awk '{print $1}')
if [ -n "$DBG_KB" ] && [ "$DBG_KB" -gt 5242880 ]; then   # > 5 GiB
  echo "  ⚠ target/debug 已 $(du -sh target/debug 2>/dev/null | awk '{print $1}')（> 5 G）"
  echo "     建议：rm -rf target/debug/{deps,build,incremental}"
  echo "     （release 保留；本场实测 9.5 G → 614 M、磁盘 63% → 60%）"
fi

if [ "$fail" -ne 0 ]; then
  echo "  ⇒ **不许提交**（上面有红）"
  exit 1
fi
echo "  ✓ 可以提交"
