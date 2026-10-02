#!/usr/bin/env bash
# **抓取 CC0 纸张/画布纹理到工作区缓存** ✓（用户裁定：下载到本地缓存 ✓，不入 git ✗）。
#
# **为什么不把它们签进仓库** ✗：ambientCG 最小的 `1K-PNG` 就有 **13.6MB** ✓（API 实测 ✓）
# ⇒ 几十张就是几百 MB ✗ ⇒ 会把仓库撑爆 ✓。所以权威副本在**工作区**里 ✓、由**本脚本**填充 ✓、
# 由**工具**（`list_textures` ✓）列给 **MCP 与 Web** 用 ✓。
#
# **为什么用 curl/unzip 而不是应用自己去下** ✓：应用是**零外部依赖**的 ✓，
# 而 `https://` 需要 TLS ✗（手写 HTTP 栈里没有 ✓）⇒ 把"取"这件事交给**系统工具** ✓、
# 把"用"这件事留在**工具层** ✓ —— 这样应用**照旧离线可跑** ✓。
#
# **许可证** ✓：ambientCG 全部资源是 **CC0** ✓（https://ambientcg.com/license/ ✓）
# ⇒ 无需署名 ✓、可商用 ✓。本脚本会把这些写进缓存目录的 `NOTICE.md` ✓。
#
# 用法：
#   scripts/fetch-textures.sh --root <工作区目录>            # 抓默认那几张纸张
#   scripts/fetch-textures.sh --root <工作区目录> Paper006 Cardboard001
#   scripts/fetch-textures.sh --root <工作区目录> --list     # 只看缓存里现已有什么
set -euo pipefail

root=""
dir="textures"
list_only=0
ids=()
while [ $# -gt 0 ]; do
  case "$1" in
    --root) root="$2"; shift 2 ;;
    --dir) dir="$2"; shift 2 ;;
    --list) list_only=1; shift ;;
    -h|--help) sed -n '2,22p' "${BASH_SOURCE[0]}"; exit 0 ;;
    -*) echo "未知参数：$1" >&2; exit 2 ;;
    *) ids+=("$1"); shift ;;
  esac
done

if [ -z "$root" ]; then
  echo "必须给 --root（服务器启动时那个工作区目录 ☞ 缓存放在它的 ${dir}/ 下）" >&2
  exit 2
fi
cache="${root}/${dir}"

if [ "$list_only" = 1 ]; then
  if [ ! -d "$cache" ]; then
    echo "缓存还不存在：${cache}（先不带 --list 跑一次 ✓）"
    exit 0
  fi
  echo "缓存 ${cache} 里现有："
  # **不要用 `find -printf`** ✗（GNU 专属 ✓；macOS 的 BSD find 没有它 ✓ ⇒ 会直接报错 ✓）。
  # 换成"逐行 + `wc -c`" ✓ —— 两边都能跑 ✓（真实用户报告：他在 macOS 上跑 ✓）。
  find "$cache" -maxdepth 1 -type f -name '*.png' | sort | while IFS= read -r file; do
    printf '  %s  %s 字节\n' "$(basename "${file}")" "$(wc -c < "${file}")"
  done
  exit 0
fi

# **默认那几张** ✓：都是本脚本核对过、确实提供 `1K-PNG` 的资产 ✓（从 API 的 CSV 里读出来的 ✓）。
if [ ${#ids[@]} -eq 0 ]; then
  ids=(Paper001 Paper003 Paper004 Cardboard001)
fi

for tool in curl unzip; do
  command -v "$tool" >/dev/null 2>&1 || { echo "需要 ${tool}（系统工具 ✓）" >&2; exit 3; }
done

mkdir -p "$cache"
for id in "${ids[@]}"; do
  target="${cache}/${id}.png"
  if [ -f "$target" ]; then
    echo "已有 ⇒ 跳过 ${id}"
    continue
  fi
  url="https://ambientCG.com/get?file=${id}_1K-PNG.zip"
  tmp="$(mktemp -d)"
  # **失败要留痕** ✗（网络断了不能静默跳过 ✓ —— 否则用户以为抓完了 ✓）。
  if ! curl -fsSL "$url" -o "${tmp}/pack.zip"; then
    echo "✗ 下载失败：${url}" >&2
    rm -rf "$tmp"
    continue
  fi
  unzip -q -o "${tmp}/pack.zip" -d "$tmp"
  # **要的是颜色贴图** ✓（压缩包里还有法线/粗糙度等 ✓ —— 我们只要 Color ✓）。
  color="$(find "$tmp" -iname '*Color*.png' | head -1)"
  if [ -z "$color" ]; then
    color="$(find "$tmp" -iname '*.png' | head -1)"
  fi
  if [ -z "$color" ]; then
    echo "✗ 包里没找到 PNG：${id}" >&2
    rm -rf "$tmp"
    continue
  fi
  # **入库前自检 PNG 头** ✗（真实事故 ✓）：本项目解码器只吃 **8 位、非隔行、RGB/RGBA** ✓
  # ⇒ 几张素材是**灰度** ✓（`Paper003` 就是 ✓）⇒ 抓回来也**用不了** ✗
  # ⇒ "抓到了却不能用" 与 "说能用其实不能用" 是同一类病 ✓ ⇒ 这里**当场判定并说清** ✓。
  if command -v python3 >/dev/null 2>&1; then
    verdict="$(python3 - "$color" <<'PYEOF'
import struct, sys
data = open(sys.argv[1], 'rb').read(29)
if data[:8] != b'\x89PNG\r\n\x1a\n' or data[12:16] != b'IHDR':
    print('not-png'); raise SystemExit
_, _, depth, color, _, _, interlace = struct.unpack('>IIBBBBB', data[16:29])
print('ok' if (depth == 8 and interlace == 0 and color in (2, 6)) else 'unusable')
PYEOF
)"
    if [ "$verdict" != "ok" ]; then
      echo "⚠️  ${id} 的 PNG 本项目解码器吃不了（$verdict ⇒ 需 8 位非隔行 RGB/RGBA）⇒ 跳过 ✓" >&2
      rm -rf "$tmp"
      continue
    fi
  fi
  cp "$color" "$target"
  rm -rf "$tmp"
  echo "已取 ${id} ⇒ ${target}（$(du -h "$target" | cut -f1)）"
done

cat > "${cache}/NOTICE.md" <<'NOTICE'
# 纹理来源与许可 —— **CC0 1.0** ✓（公共领域贡献 ✓，无需署名 ✓、可商用 ✓）

这些纹理由 `scripts/fetch-textures.sh` 从 **ambientCG** 抓取：
<https://ambientcg.com/> ✓，许可证说明见 <https://ambientcg.com/license/> ✓。
抓的是各资产的 **1K-PNG** 变体里的**颜色贴图**（Color ✓）。

**本目录不在 git 里** ✓ —— 它是**运行时缓存** ✓，每个工作区各自一份 ✓。

**为什么只收 PNG** ✓：本项目的像素解码器是**自己写的** ✓、只解 PNG ✗
（JPEG / WebP 会被**明确拒绝**并说明原因 ✓）⇒ 抓 PNG 变体是为了"抓到就能用" ✓。
NOTICE
echo "已写 ${cache}/NOTICE.md ✓"
