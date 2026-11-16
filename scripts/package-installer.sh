#!/usr/bin/env bash
# **平台安装包打包** —— 把 `scripts/package-release.sh` 产出的发行树再打成
# 「双击/一条命令就能装」的正式安装包：Windows `.msi`、macOS `.dmg`、Linux `.deb`。
#
# **为什么单独一个脚本**：release.yml 与 nightly.yml 都要它 ⇒ 同一段逻辑只写一处，
# 两条发布线共用（本仓库吃过「两条路径漂移」的亏）。原有 tar.gz 归档**不动**，
# 安装包是**多出来的一份产物**。
#
# **输入是发行树，不是二进制**：安装包的内容与 tar.gz **逐文件相同**（同一棵 stage 树），
# 只多三样平台启动器（Windows 的 `yanshi.cmd`/`yanshi-open.cmd`/`yanshi-open.ps1`、
# macOS 的 `Yanshi.app`、Linux 的 `yanshi-open`），因为「装完能跑起来」靠的是它们。
# 发行包里的 `yanshi.sh` 在 Windows 上是 bash 脚本 ⇒ Windows 用户没有它就起不来服务，
# 所以 MSI 的 payload 里补上等价的 .cmd/.ps1（开关与 yanshi.sh 逐个对应）。
#
# 用法：
#   scripts/package-installer.sh --archive dist/yanshi-….tar.gz [--out dist]
#   scripts/package-installer.sh --stage  dist/yanshi-…          [--out dist]
#   scripts/package-installer.sh --print-name --target <triple> [--version <x.y.z>]
#
# **目标与版本的权威来源**：归档/stage 里的 `BUILD-INFO`（`target:` / `version:` 两行，
# 由 package-release.sh 写）⇒ `--target` / `--version` 只作覆盖，缺省不猜。
# `--print-name` 例外：它要在「还没有包」时也能回答（后续 Homebrew cask / AUR 的
# PKGBUILD 靠它拼下载 URL）⇒ 版本缺省读仓库根 Cargo.toml。
#
# **产物名稳定**：`yanshi-<版本>-<target>.msi|.dmg|.deb` —— **不带 commit、不带日期**。
# tar.gz 的名里带 commit 是为了排查「测的是哪一版」；安装包的名是给**下载方按模板拼 URL**
# 用的（cask/PKGBUILD 钉 sha256 时必须能提前算出文件名）⇒ 两种命名各有其职，不互相迁就。
# 版本号在产物里仍可核对：包内 `BUILD-INFO` 原样保留。
#
# **校验和**：构建成功后把产物的一行 sha256 **追加**到 `<out>/SHA256SUMS` ⇒
# 两条发布线既有的「合并各腿 SHA256SUMS」路自动带上安装包；发布侧的计数断言
# 已同步改成「条目数 == 归档数 + 安装包数」。
#
# **签名现状（如实）**：MSI 无 Authenticode 签名；DMG 里的 .app 是 ad-hoc 签名
# （`codesign --sign -`），未做 Developer ID 签名与公证 ⇒ 首次打开要用户手动放行。
# 这与 tar.gz 的现状一致，不是安装包新增的降级；发布说明里也照此写。
#
# **平台工具缺了就响亮失败**：WiX（heat/candle/light）、hdiutil/codesign、dpkg-deb
# 任一缺失 ⇒ 非零退出并说清缺哪个、怎么装。绝不产出一个「看起来像安装包」的空壳。
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# sha256 工具按平台选（与 package-release.sh 同款：macOS 没有 sha256sum）。
if command -v sha256sum >/dev/null 2>&1; then
  sha256_of() { sha256sum "$1"; }
elif command -v shasum >/dev/null 2>&1; then
  sha256_of() { shasum -a 256 "$1"; }
else
  sha256_of() { echo "没有 sha256 工具（sha256sum / shasum 都没有）" >&2; return 1; }
fi

die() { echo "✗ $*" >&2; exit 1; }

archive=""
stage=""
out=""
target=""
version=""
print_name=0
while [ $# -gt 0 ]; do
  case "$1" in
    --archive) archive="$2"; shift 2 ;;
    --stage) stage="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --target) target="$2"; shift 2 ;;
    --version) version="$2"; shift 2 ;;
    --print-name) print_name=1; shift ;;
    -h|--help) awk 'NR>1 && /^#/ {print; next} NR>1 {exit}' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "未知参数：$1" >&2; exit 2 ;;
  esac
done

# **目标 → 安装包种类**：这是「哪个平台出哪种包」的唯一判处；
# 判据 check-installer-package.mjs 会逐个目标核对它（经 --print-name）。
installer_kind() {
  case "$1" in
    *-pc-windows-*) echo msi ;;
    *-apple-darwin) echo dmg ;;
    *-unknown-linux-*) echo deb ;;
    *) return 1 ;;
  esac
}
# **目标 → DEB 架构名**（dpkg 的命名与 Rust triple 不同：x86_64→amd64、aarch64→arm64）。
deb_arch() {
  case "$1" in
    x86_64-*) echo amd64 ;;
    aarch64-*) echo arm64 ;;
    *) return 1 ;;
  esac
}

# **版本缺省来源**：仓库根 Cargo.toml 的第一个 version（与 package-release.sh 同读法）。
repo_version() {
  grep -m1 '^version' "$repo/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/'
}

# ---------------------------------------------------------------- --print-name
# 只回答「这个版本＋这个目标的安装包叫什么名」：不读归档、不查平台工具 ⇒ 任何机器可跑。
if [ "${print_name}" = 1 ]; then
  [ -n "${target}" ] || die "--print-name 需要 --target <triple>"
  kind="$(installer_kind "${target}")" || die "目标 ${target} 没有对应的安装包种类（只支持 windows→msi、apple-darwin→dmg、linux→deb）"
  [ -n "${version}" ] || version="$(repo_version)"
  [ -n "${version}" ] || die "读不到版本号（Cargo.toml 里没有 version）"
  printf 'yanshi-%s-%s.%s\n' "${version}" "${target}" "${kind}"
  exit 0
fi

# ---------------------------------------------------------------- 输入与事实
[ -n "${archive}" ] || [ -n "${stage}" ] || die "需要 --archive <tar.gz> 或 --stage <dir>（或 --print-name）"
[ -n "${out}" ] || out="$repo/dist"

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

if [ -n "${archive}" ]; then
  [ -f "${archive}" ] || die "归档不存在：${archive}"
  mkdir -p "${work}/unpacked"
  tar -xzf "${archive}" -C "${work}/unpacked"
  stage="$(find "${work}/unpacked" -maxdepth 1 -type d -name 'yanshi-*' | head -1)"
  [ -n "${stage}" ] || die "归档里找不到 yanshi-* 目录：${archive}"
fi
[ -d "${stage}" ] || die "stage 目录不存在：${stage}"

# BUILD-INFO 是包内事实（package-release.sh 写入）：target/version 从它读，命令行只覆盖。
if [ -f "${stage}/BUILD-INFO" ]; then
  [ -n "${target}" ] || target="$(sed -n 's/^target: //p' "${stage}/BUILD-INFO" | head -1)"
  [ -n "${version}" ] || version="$(sed -n 's/^version: //p' "${stage}/BUILD-INFO" | head -1)"
fi
[ -n "${target}" ] || die "不知道目标平台（BUILD-INFO 里没有 target:，也没给 --target）"
[ -n "${version}" ] || die "不知道版本号（BUILD-INFO 里没有 version:，也没给 --version）"
kind="$(installer_kind "${target}")" || die "目标 ${target} 没有对应的安装包种类（只支持 windows→msi、apple-darwin→dmg、linux→deb）"

bin_suffix=""
case "${target}" in *-pc-windows-*) bin_suffix=".exe" ;; esac
[ -f "${stage}/bin/yanshi-serve${bin_suffix}" ] || die "stage 里缺 bin/yanshi-serve${bin_suffix} ⇒ 这不是一棵完整的发行树：${stage}"
[ -f "${stage}/yanshi.sh" ] || die "stage 里缺 yanshi.sh 启动器 ⇒ 这不是一棵完整的发行树：${stage}"

name="yanshi-${version}-${target}.${kind}"
mkdir -p "${out}"
outfile="${out}/${name}"
rm -f "${outfile}"

echo "==> 打安装包 ${name}（源：${stage}）"

# resize_png <源> <目标> <边长>：把 PNG 缩成正方形。先 ImageMagick
# （magick 或 convert），再退 python3 + PIL；都没有 ⇒ 返回非 0（调用方响亮警告，
# 不静默跳过）。
resize_png() {
  if command -v magick >/dev/null 2>&1; then
    magick "$1" -resize "$3x$3" "$2"
  elif command -v convert >/dev/null 2>&1; then
    convert "$1" -resize "$3x$3" "$2"
  elif python3 -c 'import PIL' >/dev/null 2>&1; then
    python3 - "$1" "$2" "$3" <<'PYEOF'
import sys
from PIL import Image
im = Image.open(sys.argv[1])
im.resize((int(sys.argv[3]), int(sys.argv[3]))).save(sys.argv[2])
PYEOF
  else
    return 1
  fi
}

# ---------------------------------------------------------------- Linux .deb
build_deb() {
  command -v dpkg-deb >/dev/null 2>&1 || die "缺 dpkg-deb ⇒ 装它：apt-get install -y dpkg-dev（GitHub 的 ubuntu runner 自带）"
  local arch root size_kb
  arch="$(deb_arch "${target}")" || die "目标 ${target} 映射不到 DEB 架构名"
  root="${work}/deb-root"
  mkdir -p "${root}/DEBIAN" "${root}/usr/bin" "${root}/usr/share/applications"
  # 发行树整棵进 /usr/lib/yanshi（布局与 tar.gz 内一致 ⇒ yanshi.sh 的相对路径推导照常成立）。
  mkdir -p "${root}/usr/lib"
  cp -R "${stage}" "${root}/usr/lib/yanshi"
  # 「起服务＋开查看器」的启动器（.desktop 与菜单走它）：已在跑就不重复起。
  cat > "${root}/usr/lib/yanshi/yanshi-open" <<'OPEN'
#!/usr/bin/env bash
# 起 yanshi-serve（没在跑才起）⇒ 等 /health ⇒ 打开查看器。
set -euo pipefail
port=8110
data="${XDG_DATA_HOME:-$HOME/.local/share}/yanshi"
mkdir -p "${data}"
healthy() { curl -sf "http://127.0.0.1:${port}/health" >/dev/null 2>&1; }
if ! healthy; then
  nohup /usr/lib/yanshi/yanshi.sh --bind "127.0.0.1:${port}" --root "${data}" \
    >>"${data}/serve.log" 2>&1 &
  for _ in $(seq 1 40); do healthy && break; sleep 0.5; done
fi
if command -v xdg-open >/dev/null 2>&1; then
  xdg-open "http://127.0.0.1:${port}/" >/dev/null 2>&1 || true
else
  echo "查看器地址：http://127.0.0.1:${port}/"
fi
OPEN
  chmod 0755 "${root}/usr/lib/yanshi/yanshi-open"
  # /usr/bin 三个薄包装：都走包内 yanshi.sh（资产路径由它按自身位置拼），不另写一份参数。
  cat > "${root}/usr/bin/yanshi-serve" <<'WRAP'
#!/bin/sh
exec /usr/lib/yanshi/yanshi.sh "$@"
WRAP
  cat > "${root}/usr/bin/yanshi" <<'WRAP'
#!/bin/sh
exec /usr/lib/yanshi/yanshi.sh "$@"
WRAP
  cat > "${root}/usr/bin/yanshi-mcp" <<'WRAP'
#!/bin/sh
exec /usr/lib/yanshi/bin/yanshi-mcp "$@"
WRAP
  chmod 0755 "${root}/usr/bin/yanshi-serve" "${root}/usr/bin/yanshi" "${root}/usr/bin/yanshi-mcp"
  # .desktop 与图标：三者同名对齐 —— .desktop 文件名、StartupWMClass、Icon 名
  # 同为一个标识（与 macOS 的 bundle id 同一个反写域名 today.wangda.yanshi）。
  # Wayland 合成器与启动器按这个标识把窗口、启动项与图标对上号；Icon 用裸名，
  # 图标文件按 hicolor 规范放 SVG（scalable）＋ PNG 48/64/128/256（512 有就带）。
  local app_id="today.wangda.yanshi"
  local brand="${stage}/share/yanshi/brand"
  local hicolor="${root}/usr/share/icons/hicolor"
  cat > "${root}/usr/share/applications/${app_id}.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Yanshi
Name[zh_CN]=偃师
Comment=Start the Yanshi painting-engine server and open the web viewer
Exec=/usr/lib/yanshi/yanshi-open
Icon=${app_id}
Terminal=false
StartupNotify=true
StartupWMClass=${app_id}
Categories=Graphics;Art;
DESKTOP
  chmod 0644 "${root}/usr/share/applications/${app_id}.desktop"
  # SVG 源用渲染 PNG 图标的那张（assets/brand/render.sh 以 icon-light.svg 渲染
  # yanshi-icon-*.png），保证矢量与位图是同一个图。
  if [ -f "${brand}/svg/icon-light.svg" ]; then
    mkdir -p "${hicolor}/scalable/apps"
    install -m 0644 "${brand}/svg/icon-light.svg" "${hicolor}/scalable/apps/${app_id}.svg"
  else
    echo "⚠ 包内没有 brand/svg/icon-light.svg ⇒ .deb 不带 scalable 图标" >&2
  fi
  # PNG：有同尺寸品牌图就用它（仓库只发 256 与 512 两档），缺的尺寸从 512 缩。
  local size
  for size in 48 64 128 256; do
    mkdir -p "${hicolor}/${size}x${size}/apps"
    if [ -f "${brand}/png/yanshi-icon-${size}.png" ]; then
      install -m 0644 "${brand}/png/yanshi-icon-${size}.png" \
        "${hicolor}/${size}x${size}/apps/${app_id}.png"
    elif [ -f "${brand}/png/yanshi-icon-512.png" ] && \
         resize_png "${brand}/png/yanshi-icon-512.png" \
           "${hicolor}/${size}x${size}/apps/${app_id}.png" "${size}"; then
      :
    else
      echo "⚠ ${size}x${size} 图标生成不了（无同尺寸品牌图、无 512 源或无缩放工具）⇒ 跳过这一档" >&2
      rmdir "${hicolor}/${size}x${size}/apps" "${hicolor}/${size}x${size}" 2>/dev/null || true
    fi
  done
  if [ -f "${brand}/png/yanshi-icon-512.png" ]; then
    mkdir -p "${hicolor}/512x512/apps"
    install -m 0644 "${brand}/png/yanshi-icon-512.png" \
      "${hicolor}/512x512/apps/${app_id}.png"
  fi
  size_kb="$(du -sk "${root}/usr" | cut -f1)"
  # control 按「将来进 APT 仓库」的标准写全：仓库索引（reprepro/aptly）会逐字段读它。
  # Depends 只写启动链真的要用的：yanshi.sh 与 yanshi-open 都是 bash、健康检查用 curl；
  # 两个二进制是静态包（Linux 缺省 --static），不依赖系统 glibc 版本 ⇒ 不写 libc 依赖。
  cat > "${root}/DEBIAN/control" <<CONTROL
Package: yanshi
Version: ${version}
Architecture: ${arch}
Maintainer: The Yanshi Authors <yanshi@wangda.today>
Section: graphics
Priority: optional
Depends: bash, curl
Recommends: xdg-utils
Installed-Size: ${size_kb}
Description: Yanshi — AI-native headless painting engine and web viewer
 Yanshi (偃师) is a painting engine whose every edit is an operation
 appended to a log. This package installs the yanshi-serve HTTP service,
 the yanshi-mcp server and the bundled runtime asset trees (WASM compute
 kernel, medium plug-ins, brushes, textures and palettes) under
 /usr/lib/yanshi. Run \`yanshi-serve\` or launch Yanshi from the
 applications menu; the web viewer is at http://127.0.0.1:8110/.
CONTROL
  # 权限归一化：cp -R 会把构建环境的 umask 带进包里（本机实测 umask 007 ⇒ 目录 770、
  # 文件 660 ⇒ 装到用户机上普通用户连 /usr/lib/yanshi 都进不去）。统一钉死：
  # 目录 0755、文件按「原本可执行否」给 0755/0644。dpkg-deb 对 control 目录
  # 另有硬要求（0755–0775），也在这一并满足。
  find "${root}" -type d -exec chmod 0755 {} +
  find "${root}" -type f -perm -u=x -exec chmod 0755 {} +
  find "${root}" -type f ! -perm -u=x -exec chmod 0644 {} +
  dpkg-deb --build --root-owner-group "${root}" "${outfile}" >/dev/null
  echo "    ✓ .deb 已生成（架构 ${arch}，Installed-Size ${size_kb} KiB）"
}

# ---------------------------------------------------------------- macOS .dmg
build_dmg() {
  [ "$(uname -s)" = "Darwin" ] || die "打 .dmg 必须在 macOS 上（hdiutil/codesign 是 macOS 工具）；当前是 $(uname -s)"
  for tool in hdiutil codesign plutil ditto; do
    command -v "${tool}" >/dev/null 2>&1 || die "缺 macOS 工具：${tool}"
  done
  local app="${work}/Yanshi.app"
  mkdir -p "${app}/Contents/MacOS" "${app}/Contents/Resources"
  # 发行树整棵进 Resources/yanshi（布局与 tar.gz 内一致 ⇒ yanshi.sh 的相对路径推导照常成立）。
  ditto "${stage}" "${app}/Contents/Resources/yanshi"
  # .app 的可执行体 = 启动器脚本：起服务（没在跑才起）⇒ 等 /health ⇒ open 查看器。
  cat > "${app}/Contents/MacOS/Yanshi" <<'LAUNCH'
#!/bin/bash
# Yanshi.app 启动器：以包内资产起 yanshi-serve，并打开 Web 查看器。
set -euo pipefail
contents="$(cd "$(dirname "$0")/.." && pwd)"
pkg="${contents}/Resources/yanshi"
port=8110
data="${HOME}/Library/Application Support/Yanshi"
mkdir -p "${data}"
healthy() { curl -sf "http://127.0.0.1:${port}/health" >/dev/null 2>&1; }
if ! healthy; then
  nohup "${pkg}/yanshi.sh" --bind "127.0.0.1:${port}" --root "${data}" \
    >>"${data}/serve.log" 2>&1 &
  for _ in $(seq 1 40); do healthy && break; sleep 0.5; done
fi
open "http://127.0.0.1:${port}/"
LAUNCH
  chmod 0755 "${app}/Contents/MacOS/Yanshi"
  # 图标：品牌 PNG → .icns（iconutil 需要 512 与 256 两档；sips 现场缩放）。
  local icon_src="${stage}/share/yanshi/brand/png/yanshi-icon-512.png"
  if [ -f "${icon_src}" ] && command -v iconutil >/dev/null 2>&1 && command -v sips >/dev/null 2>&1; then
    local iconset="${work}/yanshi.iconset"
    mkdir -p "${iconset}"
    cp "${icon_src}" "${iconset}/icon_512x512.png"
    sips -z 256 256 "${icon_src}" --out "${iconset}/icon_256x256.png" >/dev/null
    iconutil -c icns "${iconset}" -o "${app}/Contents/Resources/yanshi.icns"
    echo "    ✓ 图标已生成：yanshi.icns"
  else
    echo "⚠ 缺图标源或 iconutil/sips ⇒ .app 不带图标（功能不受影响，但 Dock 里是通用图标）" >&2
  fi
  # Info.plist：仓库没有既有 bundle id ⇒ 按作者域名 wangda.today 反写 today.wangda.yanshi。
  cat > "${app}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>Yanshi</string>
  <key>CFBundleDisplayName</key>
  <string>Yanshi 偃师</string>
  <key>CFBundleIdentifier</key>
  <string>today.wangda.yanshi</string>
  <key>CFBundleVersion</key>
  <string>${version}</string>
  <key>CFBundleShortVersionString</key>
  <string>${version}</string>
  <key>CFBundleExecutable</key>
  <string>Yanshi</string>
  <key>CFBundleIconFile</key>
  <string>yanshi</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
</dict>
</plist>
PLIST
  plutil -lint "${app}/Contents/Info.plist" >/dev/null || die "Info.plist 没通过 plutil -lint"
  # ad-hoc 签名：没有 Developer ID 时唯一能做的签名；作用是让 bundle 结构完整、
  # 本机 TCC 能按 bundle id 认它。**不是**公证，首次打开仍要手动放行（如实写进发布说明）。
  codesign --force --deep --sign - "${app}"
  codesign --verify --deep --strict "${app}"
  echo "    ✓ Yanshi.app 已组装并 ad-hoc 签名（CFBundleIdentifier=today.wangda.yanshi）"
  # DMG：Yanshi.app + Applications 软链（拖拽安装的标准形态）。
  local dmg_stage="${work}/dmg-stage"
  mkdir -p "${dmg_stage}"
  ditto "${app}" "${dmg_stage}/Yanshi.app"
  ln -s /Applications "${dmg_stage}/Applications"
  hdiutil create -volname "Yanshi ${version}" -srcfolder "${dmg_stage}" \
    -ov -format UDZO "${outfile}" >/dev/null
  hdiutil verify "${outfile}" >/dev/null
  echo "    ✓ .dmg 已生成并通过 hdiutil verify"
}

# ---------------------------------------------------------------- Windows .msi
build_msi() {
  # MSI 版本号必须是纯数字三段（MSI 的 ProductVersion 规则），先断言再干活。
  printf '%s' "${version}" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$' \
    || die "版本号 ${version} 不是纯数字三段 ⇒ 进不了 MSI 的 ProductVersion（请用 --version 给一个 x.y.z）"
  # WiX v3 工具定位：优先 PATH，其次 WIX 环境变量（WiX 安装器会设），再次默认安装路径。
  local wix_bin=""
  if command -v heat.exe >/dev/null 2>&1; then
    wix_bin="$(dirname "$(command -v heat.exe)")"
  elif [ -n "${WIX:-}" ] && [ -f "${WIX}bin/heat.exe" ]; then
    wix_bin="${WIX}bin"
  else
    # 版本目录通配兜底（choco 装的可能是 3.11/3.14 任一版）。
    local cand
    for cand in "/c/Program Files (x86)"/WiX\ Toolset\ v3.*/bin; do
      if [ -f "${cand}/heat.exe" ]; then wix_bin="${cand}"; break; fi
    done
  fi
  [ -n "${wix_bin}" ] || die "找不到 WiX Toolset v3（heat/candle/light）⇒ 先装：choco install wixtoolset -y（GitHub 的 windows runner 上可装）"
  local heat="${wix_bin}/heat.exe" candle="${wix_bin}/candle.exe" light="${wix_bin}/light.exe"

  # payload = 发行树的副本 + Windows 启动器（包里原有的 yanshi.sh 是 bash，Windows 用户用不了）。
  local payload="${work}/payload"
  mkdir -p "${payload}"
  cp -R "${stage}/." "${payload}/"
  cat > "${payload}/yanshi.cmd" <<'CMD'
@echo off
rem Yanshi launcher (Windows equivalent of yanshi.sh): point the server at
rem the asset trees inside this package, wherever it was installed.
setlocal
set "HERE=%~dp0"
if exist "%HERE%share\yanshi\wasm" goto wasm
"%HERE%bin\yanshi-serve.exe" --no-wasm --medium-dir "%HERE%share\yanshi\mediums" --assets-dir "%HERE%share\yanshi" --brand-dir "%HERE%share\yanshi\brand" %*
exit /b %ERRORLEVEL%
:wasm
"%HERE%bin\yanshi-serve.exe" --wasm-dir "%HERE%share\yanshi\wasm" --medium-dir "%HERE%share\yanshi\mediums" --assets-dir "%HERE%share\yanshi" --brand-dir "%HERE%share\yanshi\brand" %*
exit /b %ERRORLEVEL%
CMD
  cat > "${payload}/yanshi-open.cmd" <<'CMD'
@echo off
rem Start the Yanshi server (if not already running) and open the viewer.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0yanshi-open.ps1"
CMD
  cat > "${payload}/yanshi-open.ps1" <<'PS1'
$ErrorActionPreference = "SilentlyContinue"
$port = 8110
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
function Test-Healthy {
    try {
        $r = Invoke-WebRequest -UseBasicParsing -Uri "http://127.0.0.1:$port/health" -TimeoutSec 2
        return $r.StatusCode -eq 200
    } catch { return $false }
}
if (-not (Test-Healthy)) {
    $root = Join-Path $env:USERPROFILE "Documents\Yanshi"
    New-Item -ItemType Directory -Force -Path $root | Out-Null
    $cmd = Join-Path $here "yanshi.cmd"
    Start-Process -FilePath $cmd -ArgumentList @("--bind", "127.0.0.1:$port", "--root", "`"$root`"") -WindowStyle Minimized
    for ($i = 0; $i -lt 40; $i++) {
        if (Test-Healthy) { break }
        Start-Sleep -Milliseconds 500
    }
}
Start-Process "http://127.0.0.1:$port/"
PS1
  # .cmd 必须是 CRLF 行尾（cmd.exe 对 LF-only 的批处理文件解析不可靠）。
  for f in "${payload}/yanshi.cmd" "${payload}/yanshi-open.cmd"; do
    sed -i -e 's/\r$//' -e 's/$/\r/' "$f"
  done

  # 路径转换：heat/candle/light 是原生 Windows 程序，Git Bash 的 /tmp/... 路径
  # 传给它们不可靠 ⇒ 一律用 cygpath -w 换成 Windows 路径再传（cygpath 是 Git Bash 自带）。
  winpath() { if command -v cygpath >/dev/null 2>&1; then cygpath -w "$1"; else printf '%s' "$1"; fi; }
  # heat 收割 payload 目录 → 组件组（文件数由资产树决定，逐个手写不现实 ⇒ 用官方收割工具）。
  local obj="${work}/wixobj"
  mkdir -p "${obj}"
  "${heat}" dir "$(winpath "${payload}")" -cg PayloadGroup -dr INSTALLFOLDER -ag -scom -sreg -srd \
    -var var.PayloadDir -out "$(winpath "${work}/payload.wxs")"
  # 主 .wxs：产品身份、升级规则、安装目录、开始菜单快捷方式。
  # UpgradeCode 固定（44BA4266-…）：换了它，升级就会变成并排安装、卸载项也会重复。
  cat > "${work}/yanshi.wxs" <<'WXS'
<?xml version="1.0" encoding="UTF-8"?>
<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">
  <Product Id="*" Name="Yanshi" Language="1033" Version="$(var.ProductVersion)"
           Manufacturer="The Yanshi Authors" UpgradeCode="44BA4266-F97F-4AF8-BC9C-9093FE725B6B">
    <Package InstallerVersion="500" Compressed="yes" InstallScope="perMachine"
             Description="Yanshi - AI-native headless painting engine and web viewer"
             Manufacturer="The Yanshi Authors" />
    <MajorUpgrade DowngradeErrorMessage="A newer version of Yanshi is already installed." />
    <MediaTemplate EmbedCab="yes" />
    <Feature Id="MainFeature" Title="Yanshi" Level="1">
      <ComponentGroupRef Id="PayloadGroup" />
      <ComponentRef Id="StartMenuShortcut" />
    </Feature>
    <Directory Id="TARGETDIR" Name="SourceDir">
      <Directory Id="ProgramFiles64Folder">
        <Directory Id="INSTALLFOLDER" Name="Yanshi" />
      </Directory>
      <Directory Id="ProgramMenuFolder">
        <Directory Id="YanshiMenuFolder" Name="Yanshi">
          <Component Id="StartMenuShortcut" Guid="C900D446-DF2D-4948-95E1-B7A4D1115222">
            <Shortcut Id="YanshiStartShortcut" Name="Yanshi" Icon="YanshiIcon"
                      Description="Start the Yanshi server and open the web viewer"
                      Target="[INSTALLFOLDER]yanshi-open.cmd" WorkingDirectory="INSTALLFOLDER" />
            <RemoveFolder Id="RemoveYanshiMenuFolder" Directory="YanshiMenuFolder" On="uninstall" />
            <RegistryValue Root="HKCU" Key="Software\Yanshi" Name="installed" Type="integer" Value="1" KeyPath="yes" />
          </Component>
        </Directory>
      </Directory>
    </Directory>
    <Icon Id="YanshiIcon" SourceFile="$(var.PayloadDir)\share\yanshi\brand\png\favicon.ico" />
  </Product>
</Wix>
WXS
  "${candle}" -dProductVersion="${version}" -dPayloadDir="$(winpath "${payload}")" \
    -out "$(winpath "${obj}")\\" "$(winpath "${work}/yanshi.wxs")" "$(winpath "${work}/payload.wxs")"
  # -sval：跳过 light 的 ICE 校验。取舍：资产文件成百上千，ICE 全量校验慢且多为
  # 样式类告警；结构正确性改由发布线里的 msiexec /a 行政安装实证（装得出、文件落地）。
  "${light}" -sval -out "$(winpath "${outfile}")" "$(winpath "${obj}/yanshi.wixobj")" "$(winpath "${obj}/payload.wixobj")"
  echo "    ✓ .msi 已生成（WiX：${wix_bin}）"
}

# ---------------------------------------------------------------- 主分派
case "${kind}" in
  deb) build_deb ;;
  dmg) build_dmg ;;
  msi) build_msi ;;
esac

[ -f "${outfile}" ] || die "构建函数跑完了，但产物不存在：${outfile}"
# 校验和追加（不是覆盖：同一 dist 里已有 tar.gz 的那一行要留着）。
( cd "${out}" && sha256_of "$(basename "${outfile}")" >> SHA256SUMS )
echo "完成：${outfile}"
echo "      已把校验和追加到 ${out}/SHA256SUMS"
du -h "${outfile}" | sed 's/^/大小：/'
