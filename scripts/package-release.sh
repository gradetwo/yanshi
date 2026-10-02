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
# 用法：`scripts/package-release.sh [--out dist] [--skip-build] [--dynamic]`
#       或直接 `make release`（推荐 ✓：它就是这条命令 ✓）
#
# **静态是默认** ✓（真实用户报告 ✓）：动态版会继承构建机的 glibc ✗
#（实测要求 **2.43** ✓，只因 `atan2f` 一个符号 ✓），到 Debian 12（2.36 ✓）**一运行就崩** ✗
# ⇒ 默认静态 ✓，让包**在任何发行版上都能跑** ✓；确实需要动态链接时才用 `--dynamic` ✓。
#
# **`--static`：打成完全静态的二进制** ✓（真实用户报告的第 1 条严重缺陷 ✓）。
# **它解决什么** ✓：动态版链接到的是**构建机的 glibc** ✗ ⇒ 实测二进制要求 `GLIBC_2.43` ✗
#（只因为 `atan2f` 一个符号 ✓），而 Debian 12 只有 2.36 ✓ ⇒ **一运行就崩** ✗
#（用户只能手改 ELF 的 `.gnu.version` ✓ —— 那不该是使用者要做的事 ✓）。
# **静态版实测** ✓：`ldd` 报 **statically linked** ✓、**不再引用任何 GLIBC 版本** ✓、
# 体积 4.3MB ⇒ 5.6MB ✓（只大 1.3MB ✓）⇒ **在任何发行版上都能跑** ✓。
#
# **注意姿势** ✓：必须**带上 `--target`** ✓ —— 否则 `+crt-static` 会一并作用到
# proc-macro 与构建脚本 ✓ ⇒ 报 `cannot produce proc-macro …` ✗（我第一次就栽在这 ✓）。
# 带上 `--target` 后，产物落在 `target/<target>/release/` ✓（与动态构建**互不覆盖** ✓）。
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="$repo/dist"
skip_build=0
static_build=1   # **静态是默认** ✓（见文件头的说明 ✓）
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    # **`--skip-build` 只跳过"大件编译"（两个二进制）** ✓；
    # **wasm 内核与介质插件仍会刷新** ✓ —— 它们是**打包的一部分** ✓、且各自只要十几秒/一秒 ✓
    # ⇒ 为省这点时间而往包里装旧内核 ✗，是**明显不划算**的 ✓。
    --skip-build) skip_build=1; shift ;;
    --static) static_build=1; shift ;;   # 已是默认 ✓，保留是为了让脚本可读 ✓
    --dynamic) static_build=0; shift ;;  # **退出开关** ✓：确实需要动态链接时才用 ✓
    -h|--help) sed -n '2,20p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "未知参数：$1" >&2; exit 2 ;;
  esac
done

version="$(grep -m1 '^version' "$repo/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')"
target="$(rustc -vV | sed -n 's/^host: //p')"
# **commit 也进包名** ✓（用户提的排查建议 ✓）：测试者拿到包就能一眼知道是哪一版 ✓；
# 工作区有未提交改动时加 `-dirty` ✓ —— "跑的不是那版代码" 这种事要**摆在明面上** ✓。
commit="$(git -C "$repo" rev-parse --short HEAD 2>/dev/null || echo unknown)"
if [ "$commit" != "unknown" ] && [ -n "$(git -C "$repo" status --porcelain 2>/dev/null)" ]; then
  commit="${commit}-dirty"
fi
built="$(date -u '+%Y-%m-%d %H:%M UTC')"
name="yanshi-${version}-${commit}-${target}"
stage="$out/$name"

echo "==> 打包 $name"
bin_dir="$repo/target/release"
if [ "$static_build" = 1 ]; then
  bin_dir="$repo/target/$target/release"
  name="${name}-static"
  stage="$out/$name"
fi
if [ "$skip_build" = 0 ]; then
  if [ "$static_build" = 1 ]; then
    echo "--> 构建**静态** release 二进制（带 --target ✓，否则 proc-macro 会一起被静态化 ✗）"
    (cd "$repo" && RUSTFLAGS="-C target-feature=+crt-static" \
      cargo build --release --target "$target" -p yanshi-http -p yanshi-mcp)
  else
    echo "--> 构建 release 二进制（yanshi-serve / yanshi-mcp）"
    (cd "$repo" && cargo build --release -p yanshi-http -p yanshi-mcp)
  fi
fi

# **硬要求：只有那两个二进制** ✓ —— 少了它们包**真的跑不起来** ✗，必须就地报错 ✓。
for required in "$bin_dir/yanshi-serve" "$bin_dir/yanshi-mcp"; do
  [ -e "$required" ] || { echo "缺少必需产物：$required" >&2; exit 1; }
done
# **软要求：wasm 计算内核与介质插件** ✓（真实用户报告 ✓：**新克隆里直接跑打包会失败** ✗ ——
# `crates/yanshi-wasm/pkg` 是**构建产物** ✓，克隆里根本没有 ✓）。
# **为什么不报错** ✓：查看器**本来就会退化为服务端渲染** ✓（`--no-wasm` 就是这条路 ✓）
# ⇒ **打出一个"能跑、但没有浏览器端内核"的包** ✓ 比**什么都不给**有用得多 ✓。
# **但必须说清楚** ✗：缺了什么、怎么补 ✓ ⇒ 下面打印 ✓，并写进包里的 `BUILD-INFO` ✓。
# **先尝试把 wasm 内核建出来** ✓ —— 这才是治本 ✓。
#
# **为什么需要这一步** ✓（真实用户报告 ✓）：用户从**新克隆**打包 ✓ ⇒
# `crates/yanshi-wasm/pkg` 是**构建产物** ✗ ⇒ 克隆里没有 ✓ ⇒ 于是他拿到一个
# **没有浏览器内核**的包 ✗ ⇒ 查看器退化为服务端渲染 ✓ ⇒
# 症状是"**点眼睛/锁画布变白**"✗、"**找不到笔触与介质切换**"✗ ——
# **看起来像三个 bug ✓，其实是同一件事** ✓。
# **上一轮我只做了"警告后继续"** ✗ ⇒ 包能用了 ✓ 但**少一半功能** ✗ ⇒ **那不够** ✓：
# 能建就该**当场建** ✓（`make release` 的语义就是"给我一个能用的包" ✓）。
# **每次打包都重建内核** ✓（原来是"**缺了才建**"✗）。
#
# **为什么改** ✓：上一版只在 `pkg/` **不存在**时才建 ✓ ⇒ 若它存在但**过期** ✗
# （比如它是上一个提交编的 ✓），就会把一个**旧内核**装进**新名字**的包 ✓
# —— 与"二进制里的 commit 冻住"是**完全同一类缺陷** ✗（"我提交了" ≠ "产物是我提交的那版" ✓）。
# **代价只有十几秒** ✓（实测 14s ✓）⇒ **没有理由为省这点时间冒错包的风险** ✗。
#
# **这个步骤不受 `--skip-build` 影响** ✓（那是"跳过大件编译"的开关 ✓，
# 而内核与介质是**打包的一部分** ✓，且很便宜 ✓ ⇒ 它们总是保持最新 ✓）。
echo "--> 构建 / 刷新 wasm 计算内核（每次 ✓）"
# **工具链**：本机实测 wasm32 目标装在 rustup 工具链里 ✓ ⇒ 挑一个带它的 ✓
#（`rustup` 命令本身可能不在 PATH 上 ✓ —— 这个坑我在 `wasm-smoke.sh` 里踩过 ✓）。
kernel_toolchain=""
for candidate in "$HOME"/.rustup/toolchains/*/; do
  if [ -d "${candidate}lib/rustlib/wasm32-unknown-unknown" ] && [ -x "${candidate}bin/cargo" ]; then
    kernel_toolchain="$candidate"
    break
  fi
done
# **`wasm-bindgen`**：它常装在 `~/.cargo/bin` ✓ 而**不在 PATH** 上 ✗
#（实测：`command -v wasm-bindgen` 说没有 ✓，其实装着 ✓ —— 又是同一个坑 ✓）。
bindgen_bin="$(command -v wasm-bindgen 2>/dev/null || true)"
if [ -z "$bindgen_bin" ] && [ -x "$HOME/.cargo/bin/wasm-bindgen" ]; then
  bindgen_bin="$HOME/.cargo/bin/wasm-bindgen"
fi
if [ -n "$kernel_toolchain" ] && [ -n "$bindgen_bin" ]; then
  echo "    用 ${kernel_toolchain} 编译 ✓，再用 ${bindgen_bin} 生成绑定 ✓"
  if PATH="${kernel_toolchain}bin:$PATH" "${kernel_toolchain}bin/cargo" build \
       --manifest-path "$repo/Cargo.toml" --release \
       --target wasm32-unknown-unknown -p yanshi-wasm >/dev/null 2>&1 \
     && "$bindgen_bin" --target web \
       --out-dir "$repo/crates/yanshi-wasm/pkg" \
       "$repo/target/wasm32-unknown-unknown/release/yanshi_wasm.wasm" >/dev/null 2>&1; then
    echo "    ✓ 内核已生成：$(du -h "$repo/crates/yanshi-wasm/pkg/yanshi_wasm_bg.wasm" | cut -f1)"
  else
    echo "    ✗ 内核构建失败 ⇒ 按下面缺产物处理" >&2
  fi
else
  echo "    ⚠️ 缺工具 ⇒ 无法现场构建："
  [ -n "$kernel_toolchain" ] || echo "       · 找一个装了 wasm32-unknown-unknown 的 rustup 工具链"
  [ -n "$bindgen_bin" ] || echo "       · cargo install wasm-bindgen-cli"
fi


# **介质插件也要重建** ✓ —— 它们**提交在 `assets/mediums/`** ✓，
# 而此前的打包**完全不碰它们** ✗ ⇒ 改了插件源码却只重编二进制 ✓
# ⇒ 浏览器端会拿到**旧插件** ✗（我上次是**手工**重建 `oil.wasm` 的 ✗ ⇒ 那不可靠 ✓）。
# **代价** ✓：6 个小 crate ✓ 每个约 1 秒 ✓。
if [ -n "$kernel_toolchain" ] && [ -n "$bindgen_bin" ]; then
  echo "--> 重建六个介质插件（wasm ✓）"
  if PATH="${kernel_toolchain}bin:$PATH" "${kernel_toolchain}bin/cargo" build \
       --manifest-path "$repo/Cargo.toml" --release --target wasm32-unknown-unknown \
       -p yanshi-medium-oil -p yanshi-medium-watercolor -p yanshi-medium-marker \
       -p yanshi-medium-pencil -p yanshi-medium-pixel -p yanshi-medium-example >/dev/null 2>&1; then
    wasm_release="$repo/target/wasm32-unknown-unknown/release"
    copied=0
    # **名字要照资产表** ✓（`example` 在资产里叫 `example-dab` ✓ —— 这是历史命名 ✓）。
    cp_if_newer() {
      if [ -f "$1" ]; then cp "$1" "$2" && copied=$((copied + 1)); fi
    }
    cp_if_newer "$wasm_release/yanshi_medium_oil.wasm" "$repo/assets/mediums/oil.wasm"
    cp_if_newer "$wasm_release/yanshi_medium_watercolor.wasm" "$repo/assets/mediums/watercolor.wasm"
    cp_if_newer "$wasm_release/yanshi_medium_marker.wasm" "$repo/assets/mediums/marker.wasm"
    cp_if_newer "$wasm_release/yanshi_medium_pencil.wasm" "$repo/assets/mediums/pencil.wasm"
    cp_if_newer "$wasm_release/yanshi_medium_pixel.wasm" "$repo/assets/mediums/pixel.wasm"
    cp_if_newer "$wasm_release/yanshi_medium_example.wasm" "$repo/assets/mediums/example-dab.wasm"
    echo "    已刷新 ${copied} 个介质插件 ✓（它们是提交进仓库的资产 ✓ ⇒ 内容若有变请一并提交 ✓）"
  else
    echo "    ⚠️ 介质插件重建失败 ⇒ 继续用仓库里现有的资产（可能落后于源码 ✗）" >&2
  fi
fi

missing_soft=""
[ -e "$repo/crates/yanshi-wasm/pkg" ] || missing_soft="${missing_soft} wasm-kernel"
[ -e "$repo/assets/mediums" ] || missing_soft="${missing_soft} mediums"
if [ -n "$missing_soft" ]; then
  echo "⚠️  包内缺少：${missing_soft}"
  echo "    **后果** ✓：查看器会**退化为服务端渲染** ✗ —— 浏览器端会少掉"
  echo "    WASM 内核支撑的那些功能（例如介质笔触的即时反馈 ✓，图层可见性切换后的即时重绘 ✓）。"
  echo "    **怎么补** ✓：装好 wasm-bindgen-cli（cargo install wasm-bindgen-cli ✓，"
  echo "    它常落在 ~/.cargo/bin ✓ 而不在 PATH 上 ✗）＋ 一个装了 wasm32-unknown-unknown 的工具链 ✓，"
  echo "    然后**重新跑** make release ✓（脚本会现场构建内核 ✓）。"
fi

rm -rf "$stage"
mkdir -p "$stage/bin" "$stage/share/yanshi"

echo "--> 组装目录树"
install -m 0755 "$bin_dir/yanshi-serve" "$stage/bin/yanshi-serve"
install -m 0755 "$bin_dir/yanshi-mcp" "$stage/bin/yanshi-mcp"
# **资产树** ✓：用 `cp -R` 保留目录结构 ✓；介质只带 `*.wasm` ✓（源码不属于运行期 ✓）。
# **存在才拷** ✓（缺失时上面已经警告过 ✓ ⇒ 这里不能因为"没有"就整个失败 ✗）。
if [ -e "$repo/crates/yanshi-wasm/pkg" ]; then
  cp -R "$repo/crates/yanshi-wasm/pkg" "$stage/share/yanshi/wasm"
fi
mkdir -p "$stage/share/yanshi/mediums"
if [ -d "$repo/assets/mediums" ]; then
  find "$repo/assets/mediums" -maxdepth 1 -name '*.wasm' -exec cp {} "$stage/share/yanshi/mediums/" \;
fi
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
# **缺 wasm 内核就明说** ✓（真实用户报告 ✓：新克隆里那个目录**根本不存在** ✗）。
# 指向一个不存在的目录 ✗ 会让服务端困惑 ✓；而 `--no-wasm` 是**设计里正路** ✓
# ⇒ "查看器退化为服务端渲染" ✓ —— 功能少一块 ✓，但**能正常跑** ✓。
wasm_args=()
if [ -d "$here/share/yanshi/wasm" ]; then
  wasm_args+=(--wasm-dir "$here/share/yanshi/wasm")
else
  wasm_args+=(--no-wasm)
  echo "提示：本包未含 WASM 计算内核 ⇒ 查看器将走服务端渲染（功能正常，浏览器端少一份内核）" >&2
fi
exec "$here/bin/yanshi-serve" \
  "${wasm_args[@]}" \
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

# **包内 BUILD-INFO** ✓：版本 / commit / 目标平台 / 构建时间 / rustc 版本 ✓。
# 排查时先看这个文件 ✓，不必再问"这是哪个 commit 的构建" ✓。
cat > "$stage/BUILD-INFO" <<INFO
name: yanshi
version: ${version}
commit: ${commit}
target: ${target}
built: ${built}
rustc: $(rustc -V)
INFO

# **打包守卫：报出这个包要求多新的 glibc** ✓ —— 用户正是在这里踩的坑 ✗：
# 动态包会**静默继承构建机的 glibc** ✓（实测 2.43 ✓），到 Debian 12（2.36 ✓）就直接崩 ✗。
# 所以这里**每次都打印** ✓；静态包则明确说"完全不依赖" ✓。
echo "--> 检查运行期依赖（glibc 要求）"
if ldd "$stage/bin/yanshi-serve" 2>&1 | grep -q "statically linked"; then
  echo "    静态链接 ✓ 不依赖任何 glibc 版本 ✓"
else
  newest="$(objdump -T "$stage/bin/yanshi-serve" 2>/dev/null | grep -oE 'GLIBC_[0-9.]+' | sort -Vu | tail -1)"
  echo "    动态链接 ⇒ 要求 ${newest}（构建机是 $(ldd --version | head -1 | grep -oE '[0-9]+\.[0-9]+$')）"
  echo "    ⚠️  比目标发行版新就会一运行就崩 ✗ ⇒ 建议加 --static 重新打包 ✓"
fi
if [ "$static_build" = 1 ]; then
  if objdump -T "$stage/bin/yanshi-serve" 2>/dev/null | grep -q "GLIBC_2"; then
    echo "    ✗ 声明了 --static 却仍引用 GLIBC ⇒ 打包失败" >&2
    exit 1
  fi
  echo "    已断言：静态包**不引用**任何 GLIBC 符号 ✓"
fi

# **断言：二进制里报的 commit 必须与包名里的一致** ✓（真实用户报告 ✓）。
#
# **为什么必须有这道断言** ✓：`build.rs` 曾经只 watch `.git/HEAD` ✗，
# 而普通分支检出下那文件只写 `ref: refs/heads/main` ✓（**分支名，不含哈希** ✗）
# ⇒ 每次提交都不改动它 ✗ ⇒ cargo 认为构建脚本仍新鲜 ✗ ⇒
# **二进制里的 `YANSHI_COMMIT` 冻在很久以前** ✗。
# 用户实测到的就是这个 ✓：包名是 `f8075f3` ✓ 而 `--version` 报 `dd2171d` ✓
# ⇒ **他根本不知道自己在测哪一版** ✗ —— 而这会让**每一次验收都可能验错对象** ✗，
# 是**最贵的一类缺陷** ✓（比功能缺失严重得多 ✓）。
# **两层防护** ✓：`build.rs` 已修好✓（watch 真正会变的 ref 文件 ✓）；
# 这里再加一道**断言** ✓ ⇒ 万一将来又出现"名字与内容不符" ✓，**包发不出去** ✗ ✓。
if ! reported="$(timeout 10 "$stage/bin/yanshi-serve" --version 2>/dev/null)"; then
  echo "    ✗ 无法运行包内二进制取版本 ⇒ 打包失败" >&2
  exit 1
fi
reported_commit="$(printf '%s' "$reported" | grep -oE 'commit [^,)]+' | sed 's/commit //')"
# 包名里的 commit 可能带 `-dirty` ✓（构建时工作区有未提交改动 ✓）⇒ 比较时去掉它 ✓。
baked="$(printf '%s' "$commit" | sed 's/-dirty$//')"
seen="$(printf '%s' "$reported_commit" | sed 's/-dirty$//')"
if [ "$seen" != "$baked" ]; then
  echo "    ✗ **包名与二进制内的 commit 不一致** ⇒ 打包失败" >&2
  echo "      包名:     ${baked} ✓" >&2
  echo "      二进制:   ${seen} ✗" >&2
  echo "      多半是二进制**没有重新编译** ✗（改了源码却复用了旧产物 ✓）。" >&2
  echo "      处理：先跑 cargo build --release -p yanshi-http -p yanshi-mcp（或删掉 target/release 里的对应产物 ✓），再重打 ✓。" >&2
  exit 1
fi
echo "    已断言：二进制内 commit（${seen}）与包名一致 ✓"

echo "--> 打包 tar.gz 与校验和"
(cd "$out" && tar -czf "$name.tar.gz" "$name")
(cd "$out" && sha256sum "$name.tar.gz" > SHA256SUMS)
echo
echo "完成：$out/$name.tar.gz"
echo "      $out/SHA256SUMS"
du -sh "$out/$name.tar.gz" | sed 's/^/大小：/'
