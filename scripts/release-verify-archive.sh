#!/usr/bin/env bash
# **包名必须与包内二进制的真实格式/架构一致**判据 ✓ —— 跨平台打包 BUG 的**根治** ✓。
#
# 真实用户报告 ✓："macOS 上打包的包名寫著 x86_64-unknown-linux-gnu，但裝的是蘋果芯片的版本，
# 跑不起來。Exec format error" ✓ —— 根因是"**名字来自变量、内容来自现有文件，且从不校验二者一致**" ✗。
# 本脚本只做一件事 ✓：**拿包名里的 triple 去核对包里那个二进制** ✓（`file` 说话 ✓），不一致就退出 1 ✗。
#
# 用法 ✓：scripts/release-verify-archive.sh <包.tar.gz> [期望 triple]
#   * 不给 triple ⇒ 从**文件名**里解析（`…-<triple>.tar.gz` ✓）；
#   * 打包脚本应在**生成包之后、对外发布之前**调它 ✓（宁可失败也不要发出名实不符的包 ✗）。
set -euo pipefail
ARCHIVE="${1:-}"
EXPECTED="${2:-}"
[ -n "${ARCHIVE}" ] && [ -f "${ARCHIVE}" ] || { echo "用法: $0 <包.tar.gz> [期望 triple]"; exit 2; }
if [ -z "${EXPECTED}" ]; then
  base="$(basename "${ARCHIVE}")"
  base="${base%.tar.gz}"; base="${base%.tgz}"
  # 形如 yanshi-<version>-<triple> ⇒ 取最后两段（`<arch>-<vendor>-<os>-<abi>` 共 3~4 段 ✓）
  EXPECTED="$(printf '%s' "${base}" | grep -oE '[a-z0-9_]+-[a-z0-9_]+-[a-z0-9_]+(-[a-z0-9_]+)?$' || true)"
fi
[ -n "${EXPECTED}" ] || { echo "✗ 无法从包名解析 triple，请显式给第二个参数"; exit 2; }
tmp="$(mktemp -d)"; trap 'rm -rf "${tmp}"' EXIT
tar -xzf "${ARCHIVE}" -C "${tmp}"
BIN="$(find "${tmp}" -type f -name 'yanshi-serve*' ! -name '*.d' | head -1)"
[ -n "${BIN}" ] || { echo "✗ 包里找不到 yanshi-serve"; tar -tzf "${ARCHIVE}" | head -10; exit 1; }
desc="$(file -b "${BIN}")"
echo "  包名声明的 triple：${EXPECTED}"
echo "  包内二进制：$(basename "${BIN}")｜file 判定：${desc:0:80}"
fail=0
case "${EXPECTED}" in
  *-linux-*)  printf '%s' "${desc}" | grep -q 'ELF'      || { echo "  ✗ 声明是 Linux(ELF) ⇒ 实际不是 ⇒ 装上去会 Exec format error ✗"; fail=1; } ;;
  *-apple-*)  printf '%s' "${desc}" | grep -q 'Mach-O'   || { echo "  ✗ 声明是 macOS(Mach-O) ⇒ 实际不是 ✗"; fail=1; } ;;
  *-windows-*) printf '%s' "${desc}" | grep -qi 'PE32'   || { echo "  ✗ 声明是 Windows(PE) ⇒ 实际不是 ✗"; fail=1; } ;;
  *) echo "  ⚠ 未知平台族 ⇒ 只校验架构" ;;
esac
case "${EXPECTED}" in
  x86_64-*)  printf '%s' "${desc}" | grep -qE 'x86-64|x86_64' || { echo "  ✗ 声明是 x86_64 ⇒ 实际不是 ✗"; fail=1; } ;;
  aarch64-*|arm64-*) printf '%s' "${desc}" | grep -qiE 'aarch64|arm64' || { echo "  ✗ 声明是 aarch64 ⇒ 实际不是 ✗"; fail=1; } ;;
esac
[ "${fail}" = 0 ] || exit 1
echo "  ✓ 名实相符：${EXPECTED} 与包内二进制一致"
