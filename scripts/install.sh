#!/bin/sh
# **偃师一键安装** ✓ —— 下载官方发布包、校验 SHA256、解包、装好两个可执行 ✓。
#
#   curl -fsSL https://raw.githubusercontent.com/gradetwo/yanshi/main/scripts/install.sh | sh
#
# **它装什么** ✓：包里的 `bin/yanshi-serve`（HTTP 服务）与 `bin/yanshi-mcp`（MCP 服务器）✓，
#   以及 `share/yanshi/*`（笔刷、介质、纸纹等**运行期资产** ✓）。两个可执行**按 `<exe>/../share/yanshi`
#   自己找资产** ✓（见 `crates/yanshi-server/src/service.rs` 的候选路径 ✓）⇒ **∴ 必须保持包内布局** ✗。
#
# **可调环境变量** ✓：
#   YANSHI_VERSION=v0.2.1    装指定版本（缺省 latest ✓）
#   YANSHI_PREFIX=~/.local   安装前缀（缺省 ~/.local ✓；可执行进 $PREFIX/bin ✓，包体进 $PREFIX/share/yanshi ✓）
#   YANSHI_BIN_DIR=…         只改可执行落点 ✓
#   YANSHI_REPO=owner/name   从别的仓装（测试用 ✓）
#   YANSHI_DRY_RUN=1         只打印要做什么，不下载不写盘 ✓
set -eu

REPO="${YANSHI_REPO:-gradetwo/yanshi}"
VERSION="${YANSHI_VERSION:-latest}"
PREFIX="${YANSHI_PREFIX:-$HOME/.local}"
BIN_DIR="${YANSHI_BIN_DIR:-$PREFIX/bin}"
DRY="${YANSHI_DRY_RUN:-0}"

say()  { printf '%s\n' "$*"; }
die()  { printf '✗ %s\n' "$*" >&2; exit 1; }

# ---- 1. 认平台 ------------------------------------------------------------------
os=$(uname -s 2>/dev/null || echo unknown)
arch=$(uname -m 2>/dev/null || echo unknown)
case "$os:$arch" in
  Linux:x86_64|Linux:amd64)   target=x86_64-unknown-linux-gnu-static ;;
  Darwin:x86_64)              target=x86_64-apple-darwin ;;
  Darwin:arm64|Darwin:aarch64) target=aarch64-apple-darwin ;;
  *)
    say "✗ 没有这个平台的预编译包：$os / $arch"
    say ""
    say "  官方目前只发布四个目标："
    say "    x86_64-unknown-linux-gnu-static   （Linux x86_64，静态链接 ✓）"
    say "    x86_64-apple-darwin              （Intel macOS）"
    say "    aarch64-apple-darwin             （Apple Silicon）"
    say "    x86_64-pc-windows-msvc           （Windows，见 README 的 PowerShell 一行）"
    say ""
    say "  Linux arm64 等其它平台请从源码编："
    say "    git clone https://github.com/$REPO && cd yanshi"
    say "    cargo build --release -p yanshi-http -p yanshi-mcp"
    exit 1
    ;;
esac

# ---- 2. 找资产 ------------------------------------------------------------------
api="https://api.github.com/repos/$REPO/releases"
if [ "$VERSION" = latest ]; then release_url="$api/latest"; else release_url="$api/tags/$VERSION"; fi

fetch() {  # fetch <url>  → stdout
  if command -v curl >/dev/null 2>&1; then curl -fsSL -m 60 "$1"
  elif command -v wget >/dev/null 2>&1; then wget -qO- "$1"
  else die "需要 curl 或 wget 之一"; fi
}

say "→ 查 $REPO 的 $VERSION 版本…"
manifest=$(fetch "$release_url") || die "拿不到 release 信息（$release_url）"

# 不依赖 jq ✓：直接把 `"browser_download_url": "…"` 抓出来再按后缀筛 ✓
urls=$(printf '%s' "$manifest" | tr ',' '\n' | grep -o 'https://[^"]*' | grep -E '\.tar\.gz$|/SHA256SUMS$' || true)
pkg_url=$(printf '%s\n' "$urls" | grep -E -- "-$target\.tar\.gz$" | head -1)
sums_url=$(printf '%s\n' "$urls" | grep -E '/SHA256SUMS$' | head -1)
[ -n "$pkg_url" ] && [ -n "$sums_url" ] || die "这一版没有 $target 的包（也许还没发布完）"

ver_tag=$(printf '%s' "$pkg_url" | sed 's|.*/yanshi-\([^-]*\)-.*|\1|')
say "  找到 yanshi $ver_tag（$target）"

if [ "$DRY" = 1 ]; then
  say "→ 干跑：会下载 $(basename "$pkg_url")"
  say "→ 干跑：会装到 $PREFIX/share/yanshi/$ver_tag，可执行软链进 $BIN_DIR"
  exit 0
fi

# ---- 3. 下载并校验 --------------------------------------------------------------
tmp=$(mktemp -d 2>/dev/null || mktemp -d -t yanshi)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "→ 下载…"
if command -v curl >/dev/null 2>&1; then curl -fsSL -m 900 -o "$tmp/pkg.tar.gz" "$pkg_url" && curl -fsSL -m 60 -o "$tmp/SHA256SUMS" "$sums_url"
else wget -q -O "$tmp/pkg.tar.gz" "$pkg_url" && wget -q -O "$tmp/SHA256SUMS" "$sums_url"; fi
[ -s "$tmp/pkg.tar.gz" ] || die "下载失败"

say "→ 校验 SHA256…"
want=$(grep -E -- "-$target\.tar\.gz$" "$tmp/SHA256SUMS" | awk '{print $1}' | head -1)
[ -n "$want" ] || die "SHA256SUMS 里没有 $target 的条目"
if command -v sha256sum >/dev/null 2>&1; then got=$(sha256sum "$tmp/pkg.tar.gz" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then got=$(shasum -a 256 "$tmp/pkg.tar.gz" | awk '{print $1}')
else die "需要 sha256sum 或 shasum 之一"; fi
[ "$want" = "$got" ] || die "校验不通过：期望 $want，实际 $got（包可能被截断或被改动 ✗）"
say "  ✓ 校验通过"

# ---- 4. 解包并装 ----------------------------------------------------------------
dest="$PREFIX/share/yanshi/$ver_tag"
say "→ 解包到 $dest"
mkdir -p "$dest"
tar -xzf "$tmp/pkg.tar.gz" -C "$dest" --strip-components=1 || die "解包失败"
[ -x "$dest/bin/yanshi-serve" ] || die "包里没有 bin/yanshi-serve"

mkdir -p "$BIN_DIR"
for name in yanshi-serve yanshi-mcp; do
  [ -x "$dest/bin/$name" ] || continue
  wrapper="$BIN_DIR/$name"
  # **用 wrapper 而不是软链** ✓：软链下 `current_exe` 在部分平台会指向软链本身 ✗，
  # 那样 `<exe>/../share/yanshi` 就找不到资产 ✗ ⇒ **∴ 明确 exec 真身** ✓。
  printf '#!/bin/sh\nexec "%s/bin/%s" "$@"\n' "$dest" "$name" > "$wrapper"
  chmod +x "$wrapper"
  say "  ✓ $wrapper"
done

# ---- 5. 收尾 --------------------------------------------------------------------
say ""
say "装好了 ✓  yanshi $ver_tag  →  $dest"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) say ""; say "  ⚠ $BIN_DIR 不在 PATH 里，加一行："
     say "     echo 'export PATH=\"$BIN_DIR:\$PATH\"' >> ~/.profile" ;;
esac
say ""
say "  跑起来（HTTP 服务 ＋ 浏览器查看器）："
say "    yanshi-serve --root ~/paint --doc mydoc --bind 127.0.0.1:8080"
say "  接到 MCP 客户端（Claude Desktop / Cursor 等）："
say "    yanshi-mcp --root ~/paint --doc mydoc"
say "  只列工具、不起服务："
say "    yanshi-mcp --list-tools --profiles core"
