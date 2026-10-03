#!/usr/bin/env bash
# 三态判据：`target_installed` 必须区分「已装 / 明确没装 / 无法判断」。
# 做法：用 sed 把函数从脚本里抽出来（不 source 整个脚本，它有副作用），再用临时 HOME 造三种情形。
set -uo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
src="${repo}/scripts/package-release.sh"
[[ -f "${src}" ]] || { echo "找不到 ${src}"; exit 1; }
tmp_func="$(mktemp)"; sed -n '/^target_installed() {/,/^}$/p' "${src}" > "${tmp_func}"
lines=$(wc -l < "${tmp_func}")
if [[ "${lines}" -lt 5 ]]; then echo "没抽到 target_installed（只拿到 ${lines} 行）"; exit 1; fi
work="$(mktemp -d)"; trap 'rm -rf "${work}" "${tmp_func}"' EXIT
status_of() { HOME="$1" bash -c "source '${tmp_func}'; target_installed test-target; echo \${?}"; }
pass=0; fail=0
check() { # 参数依次是：期望值 / 实际值 / 说明
  if [[ "${1}" == "${2}" ]]; then echo "  ok  ${3} => ${2}"; pass=$((pass + 1));
  else echo "  BAD ${3} => 期望 ${1}、实际 ${2}"; fail=$((fail + 1)); fi
}
a="${work}/a/.rustup/toolchains/stable/lib/rustlib/test-target"; mkdir -p "${a}"
check 0 "$(status_of "${work}/a")" "有该目标"
b="${work}/b/.rustup/toolchains/stable/lib/rustlib"; mkdir -p "${b}"
check 1 "$(status_of "${work}/b")" "有 rustup 但缺该目标"
mkdir -p "${work}/c"
check 2 "$(status_of "${work}/c")" "没有 rustup（无法判断）"
echo "  通过 ${pass} 条、失败 ${fail} 条"
[[ "${fail}" == 0 ]] || exit 1
