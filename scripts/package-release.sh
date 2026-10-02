#!/usr/bin/env bash
# **发布打包脚本** ✓ —— 把 release 二进制与它**运行期真正需要的资产**打成一个自包含的 tar.gz ✓。
#
# **为什么不能只发二进制** ✗：服务端启动时要读**两棵资产树** ✓：
#   * WASM 计算内核 → 缺省 `crates/yanshi-wasm/pkg` ✓（`--wasm-dir` 可改 ✓）
#   * 介质插件     → 缺省 `assets/mediums` ✓（`--medium-dir` 可改 ✓）
# 两者都是**相对当前工作目录**的默认值 ✓ ⇒ 只发二进制、又在别的目录里跑 ⇒
# 浏览器端内核与油画/水彩等插件**全都取不到** ✗（查看器会退化 ✓、`/mediums/*.wasm` 会 404 ✓）。
# ⇒ 所以包里**带上这两棵树** ✓，并附一个**包装脚本**用绝对路径把它们指回去 ✓ ——
# 这样"解包到哪都能跑" ✓，不依赖用户先 cd 到某个目录 ✓。
#
# 用法：`scripts/package-release.sh [--out dist] [--skip-build]`
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="$repo/dist"
skip_build=0
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --skip-build) skip_build=1; shift ;;
    -h|--help) sed -n '2,20p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "未知参数：$1" >&2; exit 2 ;;
  esac
done

version="$(grep -m1 '^version' "$repo/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')"
target="$(rustc -vV | sed -n 's/^host: //p')"
name="yanshi-${version}-${target}"
stage="$out/$name"

echo "==> 打包 $name"
if [ "$skip_build" = 0 ]; then
  echo "--> 构建 release 二进制（yanshi-serve / yanshi-mcp）"
  (cd "$repo" && cargo build --release -p yanshi-http -p yanshi-mcp)
fi

# 需要的产物必须先存在 ✓（缺了就地报错 ✓，不要打出一个跑不起来的包 ✗）。
for required in \
  "$repo/target/release/yanshi-serve" \
  "$repo/target/release/yanshi-mcp" \
  "$repo/crates/yanshi-wasm/pkg" \
  "$repo/assets/mediums"; do
  [ -e "$required" ] || { echo "缺少必需产物：$required" >&2; exit 1; }
done

rm -rf "$stage"
mkdir -p "$stage/bin" "$stage/share/yanshi"

echo "--> 组装目录树"
install -m 0755 "$repo/target/release/yanshi-serve" "$stage/bin/yanshi-serve"
install -m 0755 "$repo/target/release/yanshi-mcp" "$stage/bin/yanshi-mcp"
# **资产树** ✓：用 `cp -R` 保留目录结构 ✓；介质只带 `*.wasm` ✓（源码不属于运行期 ✓）。
cp -R "$repo/crates/yanshi-wasm/pkg" "$stage/share/yanshi/wasm"
mkdir -p "$stage/share/yanshi/mediums"
find "$repo/assets/mediums" -maxdepth 1 -name '*.wasm' -exec cp {} "$stage/share/yanshi/mediums/" \;
if [ -d "$repo/assets/brand" ]; then
  cp -R "$repo/assets/brand" "$stage/share/yanshi/brand"
fi
[ -f "$repo/LICENSE" ] && cp "$repo/LICENSE" "$stage/"
cp "$repo/README.md" "$stage/README.md"

# **包装脚本** ✓：用**自己所在目录**推出资产路径 ✓ ⇒ 解包到哪都能直接跑 ✓。
cat > "$stage/yanshi.sh" <<'WRAP'
#!/usr/bin/env bash
# 薄包装 ✓：把资产路径**按脚本自身位置**算出来 ✓，于是不依赖当前工作目录 ✓。
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$here/bin/yanshi-serve" \
  --wasm-dir "$here/share/yanshi/wasm" \
  --medium-dir "$here/share/yanshi/mediums" \
  --brand-dir "$here/share/yanshi/brand" \
  "$@"
WRAP
chmod 0755 "$stage/yanshi.sh"

# **包内说明** ✓（就地说明怎么用 ✓，不必上网查 ✓）。
cat > "$stage/USAGE.md" <<'USAGE'
# yanshi 发布包

自包含：二进制 + 浏览器端 WASM 计算内核 + 介质插件（油画/水彩/马克笔/铅笔/像素/示例）。

## 跑起来

    ./yanshi.sh --root ./workspace --bind 127.0.0.1:8110

然后打开 <http://127.0.0.1:8110/>（URL 里的 token 即文档 capability）。
`--root` 缺省是**纯内存**：不落地、重启即空；要留存就给它一个目录。

## 直接调二进制（不经过包装脚本）

    ./bin/yanshi-serve \
      --wasm-dir   ./share/yanshi/wasm \
      --medium-dir ./share/yanshi/mediums \
      --root ./workspace

**这两棵树必须指对**：`--wasm-dir` 是浏览器端计算内核、`--medium-dir` 是各介质插件；
指错或缺了，查看器会退化、`/mediums/*.wasm` 会 404（服务本身仍能起来，所以不会有人替你报错）。

## MCP

    ./bin/yanshi-mcp --help

## 校验

    sha256sum -c SHA256SUMS
USAGE

echo "--> 打包 tar.gz 与校验和"
(cd "$out" && tar -czf "$name.tar.gz" "$name")
(cd "$out" && sha256sum "$name.tar.gz" > SHA256SUMS)
echo
echo "完成：$out/$name.tar.gz"
echo "      $out/SHA256SUMS"
du -sh "$out/$name.tar.gz" | sed 's/^/大小：/'
