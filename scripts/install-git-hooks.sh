#!/bin/bash
# 安装 git hooks（第 1156 轮）
#
# **∴ 为什么需要**：本场我**3 次**绕过 `scripts/pre-commit.sh` 直接 commit
#   ⇒ 于是 CI 抓到：
#     · 1014 轮 `$VAR` + 非 ASCII（watch-ci.sh）
#     · 1017 轮 `declare -A`
#     · 1123 轮 `$SELF` + 非 ASCII + `read` 多变量（ci-job-durations.sh）
#   ⇒ 三次都是「本机 2 秒能发现，却留到 CI」
#
# **∴ 为什么用 hook**：「能被脚本强制的规矩会生效；只能靠记得的规矩会被违反」
#   ⇒ hook 是**唯一的强制点**（挡在 commit 之前）
#
# **∴ 用法**：`bash scripts/install-git-hooks.sh`
# **∴ 绕过**（仅紧急）：`git commit --no-verify`
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
HOOK="$ROOT/.git/hooks/pre-commit"
[ -d "$ROOT/.git/hooks" ] || { echo "${0##*/}：没有 .git/hooks ⇒ 不是 git 仓库" >&2; exit 3; }
cat > "$HOOK" <<'INNER'
#!/bin/bash
# 强制门：任何 git commit 都要过 scripts/pre-commit.sh
cd "$(git rev-parse --show-toplevel)" || exit 1
bash scripts/pre-commit.sh || {
  echo ""
  echo "  ⇒ 不许提交（pre-commit.sh 有红）"
  echo "  ⇒ 若确需绕过：git commit --no-verify"
  exit 1
}
INNER
chmod +x "$HOOK"
echo "  ✓ 已装 $HOOK"
echo "  ⇒ 它会让每次 git commit 先跑 scripts/pre-commit.sh（fmt + clippy + shell 卫生）"
