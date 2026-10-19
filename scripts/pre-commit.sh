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

if [ "$fail" -ne 0 ]; then
  echo "  ⇒ **不许提交**（上面有红）"
  exit 1
fi
echo "  ✓ 可以提交"
