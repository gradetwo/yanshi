#!/usr/bin/env bash
# **把三类内置资产装进包内资产根目录** ✓（`textures` / `brushes` / `palettes` ✓）。
#
# **为什么单独成一个脚本** ✓：这段逻辑以前**内联**在 `scripts/package-release.sh` 里 ✓，
# 于是只有"跑完整打包"（会先编译两个二进制与 wasm 内核 ✓、还会把介质插件写回仓库 ✗）
# 才能验到它 ✗ ⇒ 判据**没法直接跑这段组装逻辑** ✗。
# 抽出来之后 ✓：打包脚本与判据（`scripts/tool-package-brushes.mjs` ✓）**跑的是同一段代码** ✓
# ⇒ 判据守的是"打包真正会做的事" ✓，而不是一份随时会漂移的复制品 ✗
#（本项目反复吃过"两条路径漂移"的亏 ✓）。
#
# **为什么笔刷必须在这里** ✓：运行期把"资产根目录 + 种类子目录"拼起来 ✓
#（`crates/yanshi-http/src/server.rs` 的 `root.join("brushes").join(file)` ✓），
# 而包装脚本给服务端的资产根目录正是 `share/yanshi` ✓
# （`scripts/package-release.sh` 里的 `--assets-dir "$here/share/yanshi"` ✓）
# ⇒ 笔刷必须落在 `<stage>/share/yanshi/brushes/` ✓ —— 落在别处等于没装 ✓。
#
# **为什么"缺源目录/空源目录"必须失败** ✗（外部回归报告 ✓）：原来的写法是
# `[ -d "${source_dir}" ] || continue` ✓ —— 源目录不在就**静默跳过** ✗ ⇒
# 打出一个**没有任何笔刷**的包 ✓、退 0 报成功 ✓ ⇒ 用户拿到包才发现
# `brush_stroke` 报 `reference_not_found` ✗、手里一支可用笔刷都没有 ✗。
# 空目录同理：`copied == expected == 0` ⇒ 旧检查**照样通过** ✗（还打印"0 个文件 ✓" ✗）。
# ⇒ 现在两类都**当场失败并点名种类** ✓；装完再**逐名比对源目录** ✓
# ⇒ "产物少了东西却没人知道"这条病在这里被堵死 ✓。
#
# 用法：`scripts/stage-asset-kinds.sh <仓库根> <包内根> [种类…]`
#   包内根通常是 `<stage>` ✓ ⇒ 资产落在 `<stage>/share/yanshi/<种类>/` ✓。
#   不指定种类 ⇒ 三类都装 ✓（保持与打包脚本原来的调用一致 ✓）。
# 退出码：0 = 要求的种类都装上了且与源目录逐名一致；非 0 = 有种类缺失/为空/装少了 ✓。
set -uo pipefail
repo="${1:?用法：stage-asset-kinds.sh <仓库根> <包内根> [种类…]}"
stage="${2:?用法：stage-asset-kinds.sh <仓库根> <包内根> [种类…]}"
shift 2
# **不用数组** ✗：macOS 自带 bash 3.2 ✓，空数组配 `set -u` 在 `"${arr[@]}"` 上会报
# `unbound variable` ✗ ⇒ 这里用一个空格分隔的字符串 ✓（值来自调用方或本文件 ✓，不做 glob ✓）。
kinds="${*:-textures brushes palettes}"

# **只列"应当进包"的文件名** ✓（取 basename ✓ —— 打包是**平铺**进 `<种类>/` 的 ✓；
# 这样"两个不同子目录里的同名文件互相覆盖"这种少文件也会被下面的逐名比对抓到 ✓）。
# 跳过 macOS 的 AppleDouble（`._*` ✓）与 `.DS_Store` ✓ —— 它们本来就不该进包 ✓。
asset_names() {
  ( cd "${1}" && find . -type f ! -name '._*' ! -name '.DS_Store' | sed 's|^\./||' | while IFS= read -r p; do basename "${p}"; done | sort )
}

fail=0
for asset_kind in ${kinds}; do
  source_dir="${repo}/assets/${asset_kind}"
  target_dir="${stage}/share/yanshi/${asset_kind}"
  if [ ! -d "${source_dir}" ]; then
    echo "    ✗ ${asset_kind}：仓库里没有源目录 ${source_dir} ⇒ 装不进包" >&2
    echo "      **后果** ✓：包内缺 ${asset_kind}/ ⇒ 运行期找不到这类资产（笔刷会退成 reference_not_found）✗" >&2
    fail=1
    continue
  fi
  source_names="$(asset_names "${source_dir}")"
  if [ -z "${source_names}" ]; then
    echo "    ✗ ${asset_kind}：源目录 ${source_dir} 里一个该进包的文件都没有 ⇒ 不装一个空目录进包" >&2
    fail=1
    continue
  fi
  rm -rf "${target_dir}"
  mkdir -p "${target_dir}"
  while IFS= read -r asset_file; do
    asset_base="$(basename "${asset_file}")"
    cp "${source_dir}/${asset_file}" "${target_dir}/${asset_base}"
  done < <(cd "${source_dir}" && find . -type f ! -name '._*' ! -name '.DS_Store' | sed 's|^\./||' | sort)
  target_names="$(asset_names "${target_dir}")"
  if [ "${source_names}" != "${target_names}" ]; then
    echo "    ✗ ${asset_kind}：装进包的与源目录**逐名不一致** ⇒ 打包失败" >&2
    echo "      源目录文件数：$(printf '%s\n' "${source_names}" | grep -c . || true)" >&2
    echo "      包内文件数：  $(printf '%s\n' "${target_names}" | grep -c . || true)" >&2
    diff <(printf '%s\n' "${source_names}") <(printf '%s\n' "${target_names}") | head -20 | sed 's/^/      /' >&2
    fail=1
    continue
  fi
  count="$(printf '%s\n' "${target_names}" | grep -c . || true)"
  # **笔刷单独加一道** ✓：数量不为零还不够 ✓ —— 必须真的有 `.myb` ✓。
  # 外部报告的最终症状就是"用户一支可用笔刷都没有"✗ ⇒ 就在这里挡住 ✓。
  if [ "${asset_kind}" = "brushes" ]; then
    myb_count="$(printf '%s\n' "${target_names}" | grep -c '\.myb$' || true)"
    if [ "${myb_count}" = 0 ]; then
      echo "    ✗ brushes：包内一个 .myb 都没有 ⇒ 用户没有任何可用笔刷 ⇒ 打包失败" >&2
      fail=1
      continue
    fi
    echo "    brushes：${count} 个文件（其中 ${myb_count} 支 .myb ✓，与源目录逐名一致 ✓）"
  else
    echo "    ${asset_kind}：${count} 个文件 ✓（与源目录逐名一致 ✓）"
  fi
done
exit "${fail}"
