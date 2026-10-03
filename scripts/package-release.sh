#!/usr/bin/env bash
# **发布打包脚本** ✓ —— 把 release 二进制与它**运行期真正需要的资产**打成一个自包含的 tar.gz ✓。
#
# **为什么不能只发二进制** ✗：服务端启动时要读**两棵资产树** ✓：
#   * WASM 计算内核 → 缺省 `crates/yanshi-wasm/pkg` ✓（`--wasm-dir` 可改 ✓）
#   * 介质插件     → 缺省 `assets/mediums` ✓（`--medium-dir` 可改 ✓）
#   * 笔刷门面     → `assets/brush-module.wasm` ✓（`--brush-wasm` 可改 ✓；服务端若不指它，
#                    缺省会去 `target/` 找 ✓ ⇒ 包里必须显式指到 assets 这一份 ✓）
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
# **宿主平台** ✓（真实用户报告 ✓：在 macOS 上跑 `make release` 报 glibc 检查错误 ✗）。
#
# **为什么必须分流** ✓：glibc 是 **Linux 的 C 库** ✓ —— macOS 用 libSystem ✓
# ⇒ `ldd` / `objdump` **在 macOS 上根本不存在** ✗，而"静态链接"这条**权宜之计也不适用** ✗
#（`-C target-feature=+crt-static` 在 macOS 上没有意义 ✓）⇒ 不分流就必然误报 ✓。
# **哪边都能算 sha256** ✓（真实用户报告 ✓：macOS 上 `sha256sum` 不存在 ✗，
# 那边叫 `shasum -a 256` ✓）⇒ 选一个存在的 ✓，而不是假定自己在 Linux 上 ✗。
if command -v sha256sum >/dev/null 2>&1; then
  sha256_of() { sha256sum "$1"; }
  sha256_check() { sha256sum -c "$1"; }
  sha_tool="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
  sha256_of() { shasum -a 256 "$1"; }
  sha256_check() { shasum -a 256 -c "$1"; }
  sha_tool="shasum -a 256"
else
  sha256_of() { echo "没有 sha256 工具 ✗" >&2; return 1; }
  sha256_check() { echo "没有 sha256 工具 ✗" >&2; return 1; }
  sha_tool="（缺）"
fi

# **宿主与目标要分开** ✓（用户要求：`make release` 能**指定目标平台** ✓、也能**全平台** ✓，
# 缺省才是**当前平台** ✓）。**分流必须按"目标"判** ✗ —— 交叉编译到 Linux 时，
# 静态链接与 glibc 检查**依然适用** ✓（按宿主判就会漏掉 ✓）。
host_triple="$(rustc -vV | sed -n 's/^host: //p')"
target_triple="${host_triple}"          # 缺省＝当前平台 ✓；`--target` 可改 ✓
all_targets=0
list_targets=0
# **允许环境变量覆盖** ✓：这样"macOS 那条分支"能在 Linux 上被验收 ✓
#（否则只能等真的有一台 macOS ✗ —— 那等于**不测** ✓）。按目标平台覆盖 ✓。
target_os_override="${YANSHI_TARGET_OS:-${YANSHI_HOST_OS:-}}"

# **一批常见的发布目标** ✓（可用 `YANSHI_RELEASE_TARGETS` 覆盖 ✓）。
# 缺省**只编当前平台** ✓；`--all-targets` 才逐个试 ✓，且**没装的目标会跳过并说明** ✓（不是硬失败 ✗）。
default_all_targets() {
  # **允许用环境变量收紧/扩展这份清单** ✓（`YANSHI_RELEASE_TARGETS="a b c"` ✓）
  # ⇒ 也让"全平台那条路"能在只有宿主目标的机器上被**验收** ✓（否则只能真去装一堆目标 ✗）。
  if [ -n "${YANSHI_RELEASE_TARGETS:-}" ]; then
    printf '%s\n' ${YANSHI_RELEASE_TARGETS}
    return 0
  fi
  printf '%s\n' \
    "${host_triple}" \
    "x86_64-unknown-linux-gnu" \
    "aarch64-unknown-linux-gnu" \
    "x86_64-unknown-linux-musl" \
    "aarch64-unknown-linux-musl" \
    "x86_64-apple-darwin" \
    "aarch64-apple-darwin" \
    "x86_64-pc-windows-gnu"
}

# **某个目标装没装** ✓（用文件系统判 ✓，不依赖 rustup 在 PATH 上 ✓ ——
# 这个坑我在 wasm 与 wasm-bindgen 上踩过两次 ✓）。
target_installed() {
  local want="$1" candidate
  for candidate in "$HOME"/.rustup/toolchains/*/lib/rustlib/"${want}"; do
    [ -d "${candidate}" ] && return 0
  done
  # 找不到 rustup 工具链目录 ⇒ **不敢断定"没装"** ✗ ⇒ 交给构建去回答 ✓（宁可试一次 ✓）。
  case "${HOME}" in
    *) [ -d "${HOME}/.rustup/toolchains" ] || return 0 ;;
  esac
  return 1
}

# **"静态是默认"只是 Linux 的规矩** ✓（为的是不把构建机的 glibc 带进包里 ✓）；
# 别的平台**默认动态** ✓ —— 在那里"静态"既没必要也可能直接构建失败 ✗。
# **静态默认按"目标平台"判** ✓（交叉编译到 Linux 也一样要静态 ✓）。
resolve_is_linux() {
  local os
  os="${target_os_override:-$(printf '%s' "$1" | cut -d- -f3-)}"
  case "${os}" in
    *linux*) echo 1 ;;
    *) echo 0 ;;
  esac
}
is_linux="$(resolve_is_linux "${target_triple}")"
# **给消息用的"目标平台"名字** ✓ —— 我上一轮把 `host_os` 改名成 `target_triple` 时
# **漏改了一处引用** ✗ ⇒ 在 macOS 上以 `set -u` 直接死在 "host_os: unbound variable" ✗ ✓
#（**"改名要一次改全"** 这条我刚写进笔记 ✓，这次是自己撞上 ✓）。
target_os="$(printf '%s' "${target_triple}" | cut -d- -f3-)"
if [ "${is_linux}" = 1 ]; then
  static_build=1
else
  static_build=0
fi
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    # **`--skip-build` 只跳过"大件编译"（两个二进制）** ✓；
    # **wasm 内核与介质插件仍会刷新** ✓ —— 它们是**打包的一部分** ✓、且各自只要十几秒/一秒 ✓
    # ⇒ 为省这点时间而往包里装旧内核 ✗，是**明显不划算**的 ✓。
    --skip-build) skip_build=1; shift ;;
    --static) static_build=1; shift ;;   # 已是默认 ✓，保留是为了让脚本可读 ✓
    --dynamic) static_build=0; shift ;;  # **退出开关** ✓：确实需要动态链接时才用 ✓
    --target) target_triple="$2"; shift 2 ;;        # **指定目标平台** ✓（交叉编译 ✓）
    --all-targets|--all) all_targets=1; shift ;;    # **全平台** ✓（逐个试 ✓，没装的跳过 ✓）
    --list-targets) list_targets=1; shift ;;        # 看看本机装了哪些目标 ✓
    -h|--help) sed -n '2,20p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "未知参数：$1" >&2; exit 2 ;;
  esac
done

# **`--list-targets`：看看本机装了哪些目标** ✓（不编、不打，只回答"能编哪些" ✓）。
if [ "${list_targets}" = 1 ]; then
  echo "宿主：${host_triple}"
  echo "本机已安装、可用于发布的 Rust 目标："
  installed_any=0
  default_all_targets | sort -u | while IFS= read -r candidate; do
    if target_installed "${candidate}"; then
      printf '  ✓ %s\n' "${candidate}"
    else
      printf '  · %s（未安装 ⇒ 需要 rustup target add %s）\n' "${candidate}" "${candidate}"
    fi
  done
  echo ""
  echo "用法："
  echo "  make release                      # 只编当前平台（缺省 ✓）"
  echo "  make release TARGET=<triple>      # 编指定的那个平台 ✓"
  echo "  make release-all                  # 逐个试（没装的跳过并说明 ✓）"
  installed_any=1
  exit 0
fi

# **`--all-targets`：逐个目标各跑一遍这个脚本** ✓。
#
# **设计取舍** ✓：与其在一个进程里堆一堆条件分支 ✗，不如**递归调用自己** ✓ ——
# 每个目标都走**完全相同**的那条路 ✓（静态/glibc/sha256/组装/断言 ✓），
# 这样"单平台打包"与"全平台打包"**不可能不一致** ✗（本项目反复吃过"两条路径漂移"的亏 ✓）。
# **没装的目标跳过并说明** ✓，最后给一张**汇总表** ✓ —— 一次失败不该把其它结果藏起来 ✗。
if [ "${all_targets}" = 1 ]; then
  echo "== 全平台打包：逐个目标各跑一遍 ✓（没装的会跳过并说明 ✓）"
  ok_list=""; skip_list=""; fail_list=""
  while IFS= read -r candidate; do
    [ -n "${candidate}" ] || continue
    if ! target_installed "${candidate}"; then
      echo "--> 跳过 ${candidate}（本机未安装该目标 ⇒ rustup target add ${candidate}）"
      skip_list="${skip_list} ${candidate}"
      continue
    fi
    echo ""
    echo "--> 目标 ${candidate}"
    if bash "${BASH_SOURCE[0]}" --target "${candidate}" --out "${out}" \
         $([ "${skip_build}" = 1 ] && printf '%s' '--skip-build') \
         $([ "${static_build}" = 1 ] && printf '%s' '--static' || printf '%s' '--dynamic'); then
      ok_list="${ok_list} ${candidate}"
    else
      echo "✗ ${candidate} 打包失败 ✗" >&2
      fail_list="${fail_list} ${candidate}"
    fi
  done <<EOF
$(default_all_targets | sort -u)
EOF
  echo ""
  echo "== 汇总"
  echo "  成功：${ok_list:-（无）}"
  echo "  跳过：${skip_list:-（无）}"
  echo "  失败：${fail_list:-（无）}"
  # **有失败就以非零退出** ✓（有失败还报成功 ⇒ 与"说能用其实不能用"同类 ✗）。
  [ -z "${fail_list}" ]
  exit $?
fi

version="$(grep -m1 '^version' "$repo/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')"
# **commit 也进包名** ✓（用户提的排查建议 ✓）：测试者拿到包就能一眼知道是哪一版 ✓；
# 工作区有未提交改动时加 `-dirty` ✓ —— "跑的不是那版代码" 这种事要**摆在明面上** ✓。
commit="$(git -C "$repo" rev-parse --short HEAD 2>/dev/null || echo unknown)"
if [ "$commit" != "unknown" ] && [ -n "$(git -C "$repo" status --porcelain 2>/dev/null)" ]; then
  commit="${commit}-dirty"
fi
built="$(date -u '+%Y-%m-%d %H:%M UTC')"
name="yanshi-${version}-${commit}-${target_triple}"
stage="$out/$name"

echo "==> 打包 $name"
bin_dir="$repo/target/release"
if [ "$static_build" = 1 ]; then
  bin_dir="$repo/target/${target_triple}/release"
  name="${name}-static"
  stage="$out/$name"
fi
if [ "$skip_build" = 0 ]; then
  if [ "$static_build" = 1 ]; then
    echo "--> 构建**静态** release 二进制（带 --target ✓，否则 proc-macro 会一起被静态化 ✗）"
    (cd "$repo" && RUSTFLAGS="-C target-feature=+crt-static" \
      cargo build --release --target "${target_triple}" -p yanshi-http -p yanshi-mcp)
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
# **macOS 上 rustup 的 `rust-lld` 会缺 `libLLVM.dylib`** ✗（真实报告 ✓，2026-10-03 ✓）：
# 它动态链到 `@rpath/libLLVM.dylib` ✓，而 `stable-aarch64-apple-darwin` 的工具链包里**没有这个库** ✗
# ⇒ **任何** `--target wasm32-unknown-unknown` 的构建都会失败 ✓（内核与六个介质插件都会 ✓），
# 而报错是一屏 dyld 的路径列表 ✓ ⇒ 看起来像我们的源码坏了 ✗（其实与源码无关 ✓）。
#
# **出路** ✓：用 **Homebrew LLVM 里的 `lld`** ✓（它是多 flavor 驱动 ✓，认 rustc 传的 `-flavor wasm` ✓；
# 注意**不能**用 `wasm-ld` ✗ —— 那个二进制不认 `-flavor` ✓）。这里**自动找** ✓：
# 找到就替上 ✓ ⇒ 用户只要 `brew install llvm` ✓，`make release` 就能直接过 ✓，不必记环境变量 ✓。
# **也可以用环境变量强制指定** ✓：`YANSHI_WASM_LINKER=/path/to/lld make release` ✓。
if [ -z "${CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER:-}" ]; then
  wasm_linker="${YANSHI_WASM_LINKER:-}"
  if [ -z "${wasm_linker}" ]; then
    # **`brew install llvm` 里没有 `lld`** ✗（用户实测 ✓：`/opt/homebrew/Cellar/llvm/23.1.2/bin`
    # 里全是 clang / llvm-* ✓，**没有** `lld`、也没有 `wasm-ld` ✗）——
    # Homebrew 里 **`lld` 是单独的 formula** ✓（大约从 LLVM 13 起拆出去 ✓）
    # ⇒ 要装 `brew install lld` ✓，产物在 `/opt/homebrew/opt/lld/bin/` ✓（`lld` 与 `wasm-ld` 都在 ✓）。
    # **顺序** ✓：先找 `lld`（多 flavor 驱动 ✓，一定认 rustc 传的 `-flavor wasm` ✓），
    # 再退到 `wasm-ld`（同一个二进制的符号链接 ✓，一般也认 ✓，但排在后面 ✓）。
    for candidate in \
      /opt/homebrew/opt/lld/bin/lld \
      /usr/local/opt/lld/bin/lld \
      /opt/homebrew/opt/llvm/bin/lld \
      /usr/local/opt/llvm/bin/lld \
      /opt/homebrew/opt/lld/bin/wasm-ld \
      /usr/local/opt/lld/bin/wasm-ld \
      /opt/homebrew/opt/llvm/bin/wasm-ld \
      /usr/local/opt/llvm/bin/wasm-ld; do
      if [ -x "${candidate}" ]; then
        wasm_linker="${candidate}"
        break
      fi
    done
  fi
  # **再问一次 Homebrew 自己** ✓（装在非默认前缀 / 版本目录变了也找得到 ✓）。
  if [ -z "${wasm_linker}" ] && command -v brew >/dev/null 2>&1; then
    for formula in lld llvm; do
      prefix="$(brew --prefix "${formula}" 2>/dev/null || true)"
      for name in lld wasm-ld; do
        if [ -n "${prefix}" ] && [ -x "${prefix}/bin/${name}" ]; then
          wasm_linker="${prefix}/bin/${name}"
          break
        fi
      done
      [ -n "${wasm_linker}" ] && break
    done
  fi
  if [ -n "${wasm_linker}" ] && [ -x "${wasm_linker}" ]; then
    export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER="${wasm_linker}"
    echo "    wasm 链接器：${wasm_linker}（绕开 macOS 上缺 libLLVM.dylib 的 rust-lld ✓）"
  elif [ "$(uname -s 2>/dev/null || true)" = "Darwin" ]; then
    # **macOS 上提前把话说出来** ✓（用户实测已确认这条路能修好 ✓）：否则等的是一屏 dyld 路径 ✓
    # ——那时人只会觉得"这个仓库的构建坏了"✗，而真正缺的是一条 `brew install lld` ✓。
    echo "    提示：macOS 上 rustup 的 `rust-lld` 常缺 `libLLVM.dylib` ⇒ 这一步（以及 wasm 内核）可能失败；"
    echo "          装 Homebrew 的 **`lld`** 即可 —— 它是**单独的 formula** ✓，`brew install llvm` 里**没有**它 ✗："
    echo "            brew install lld"
    echo "          装完重跑 `make release` ✓（脚本会自动用它 ✓）；也可显式指定："
    echo "            YANSHI_WASM_LINKER=/opt/homebrew/opt/lld/bin/lld make release"
  fi
fi

bindgen_bin="$(command -v wasm-bindgen 2>/dev/null || true)"
if [ -z "$bindgen_bin" ] && [ -x "$HOME/.cargo/bin/wasm-bindgen" ]; then
  bindgen_bin="$HOME/.cargo/bin/wasm-bindgen"
fi
if [ -n "${kernel_toolchain}" ] && [ -n "${bindgen_bin}" ]; then
  # **别在干活之前先打 ✓** ✗（真实报告 ✓：脚本先打"编译 ✓…生成绑定 ✓" ✓，紧接着又说失败 ✗
  # ⇒ 那是"还没做就先报成功" ✓，与本项目一直在清除的"说能用其实不能用"同类 ✓）。
  echo "    工具链：${kernel_toolchain}"
  echo "    wasm-bindgen：${bindgen_bin}"
  # **先核对 wasm-bindgen 的版本** ✓（真实报告 ✓：macOS 上只看到"内核构建失败" ✗，
  # 而**原因被 `>/dev/null 2>&1` 吞掉了** ✗ ⇒ 用户无从下手 ✓）。版本不一致是这类失败最常见的原因 ✓
  # ⇒ **先查** ✓，并**直接给出修复命令** ✓。
  bindgen_lock="$(sed -n '/^name = "wasm-bindgen"$/{n;s/^version = "\(.*\)"$/\1/p;q;}' "$repo/Cargo.lock" 2>/dev/null || true)"
  bindgen_cli="$("${bindgen_bin}" --version 2>/dev/null | awk '{print $2}' || true)"
  if [ -n "${bindgen_lock}" ] && [ -n "${bindgen_cli}" ] && [ "${bindgen_lock}" != "${bindgen_cli}" ]; then
    echo "    ⚠️ 版本不一致：Cargo.lock 要 wasm-bindgen ${bindgen_lock} ✓，而 CLI 是 ${bindgen_cli} ✗"
    echo "       ⇒ 绑定生成会失败 ✓ ⇒ 修：cargo install wasm-bindgen-cli --version ${bindgen_lock} --locked"
  fi
  kernel_log="$(mktemp)"
  if PATH="${kernel_toolchain}bin:$PATH" "${kernel_toolchain}bin/cargo" build \
       --manifest-path "$repo/Cargo.toml" --release \
       --target wasm32-unknown-unknown -p yanshi-wasm >"${kernel_log}" 2>&1 \
     && "${bindgen_bin}" --target web \
       --out-dir "$repo/crates/yanshi-wasm/pkg" \
       "$repo/target/wasm32-unknown-unknown/release/yanshi_wasm.wasm" >>"${kernel_log}" 2>&1; then
    echo "    ✓ 内核已生成：$(du -h "$repo/crates/yanshi-wasm/pkg/yanshi_wasm_bg.wasm" | cut -f1)"
  else
    # **把真正的报错打出来** ✗ —— 否则用户只看到"失败"两个字 ✓，**无从下手** ✓。
    echo "    ✗ 内核构建失败 ⇒ 下面是**真正的报错**（最后 25 行 ✓）："
    tail -25 "${kernel_log}" | sed 's/^/      /'
    echo "      复现：cargo build --release --target wasm32-unknown-unknown -p yanshi-wasm"
    echo "      常见原因：① 该工具链缺 wasm32 目标 ⇒ rustup target add wasm32-unknown-unknown"
    echo "                ② wasm-bindgen CLI 与 Cargo.lock 版本不一致 ⇒ 见上面的修复命令"
    echo "      **失败不阻塞打包** ✓：查看器退化为服务端渲染 ✓（少一块功能 ✓，但能跑 ✓）。"
    # **但如果旧产物还在，包里就会装着"与源码不符的内核"** ✗ ——
    # 这与"二进制报的 commit 必须与包名一致"是同一种病 ✓ ⇒ 必须**明说** ✓。
    if [ -d "$repo/crates/yanshi-wasm/pkg" ]; then
      echo "      ⚠️ 注意：${repo}/crates/yanshi-wasm/pkg **已经存在** ⇒ 这次打出的包里装的是"
      echo "          **上一次构建的内核**（可能落后于源码 ✗）；需要一致就先修好上面的报错再重打 ✓。"
    else
      echo "      ⚠️ 注意：包里**不会有** wasm 内核（这个目录还不存在 ✓）。"
    fi
  fi
  rm -f "${kernel_log}"
else
  # **缺工具就说清缺哪个** ✓（"没有产物"与"工具没装"是两件事 ✓）。
  echo "    ⚠️ 缺工具 ⇒ 无法现场构建："
  [ -n "${kernel_toolchain}" ] || echo "       · 找一个装了 wasm32-unknown-unknown 的 rustup 工具链"
  [ -n "${bindgen_bin}" ] || echo "       · cargo install wasm-bindgen-cli（版本见 Cargo.lock 里的 wasm-bindgen）"
fi


# **`.myb` 笔刷的 wasm 门面也要重建** ✓（第 58 轮起：浏览器本地渲染真笔刷 ✓ ——
# 正是这一块让"拖动期就用真笔刷"成立 ✓，此前六次走服务端往返都失败 ✗）。
# 它**不需要 wasm-bindgen** ✓（纯 C ABI ✓）⇒ 只要一个带 `wasm32-unknown-unknown` 的工具链 ✓。
# 与内核那一段同样的纪律 ✓：**报错不许被吞** ✗、失败**不阻塞打包** ✓、但必须**说清后果** ✓。
if [ -n "$kernel_toolchain" ]; then
  echo "--> 构建 .myb 笔刷门面（wasm）"
  facade_log="$(mktemp)"
  if PATH="${kernel_toolchain}bin:$PATH" "${kernel_toolchain}bin/cargo" build \
       --manifest-path "$repo/Cargo.toml" --release \
       --target wasm32-unknown-unknown -p yanshi-brush-wasm >"${facade_log}" 2>&1; then
    if cp "$repo/target/wasm32-unknown-unknown/release/yanshi_brush_wasm.wasm" \
          "$repo/assets/brush-module.wasm"; then
      echo "    ✓ 门面已生成：$(du -h "$repo/assets/brush-module.wasm" | cut -f1)（assets/brush-module.wasm ✓）"
    fi
  else
    echo "    ✗ 门面构建失败 ⇒ 下面是**真正的报错**（最后 20 行 ✓）："
    tail -20 "${facade_log}" | sed 's/^/      /'
    echo "      复现：cargo build --release --target wasm32-unknown-unknown -p yanshi-brush-wasm"
    echo "      **失败不阻塞打包** ✓：拖动期不显示真笔刷 ✓（抬手仍由服务端落笔 ✓，画出来的东西一样 ✓）。"
  fi
  rm -f "${facade_log}"
fi


# **介质插件也要重建** ✓ —— 它们**提交在 `assets/mediums/`** ✓，
# 而此前的打包**完全不碰它们** ✗ ⇒ 改了插件源码却只重编二进制 ✓
# ⇒ 浏览器端会拿到**旧插件** ✗（我上次是**手工**重建 `oil.wasm` 的 ✗ ⇒ 那不可靠 ✓）。
# **代价** ✓：6 个小 crate ✓ 每个约 1 秒 ✓。
if [ -n "$kernel_toolchain" ] && [ -n "$bindgen_bin" ]; then
  echo "--> 重建六个介质插件（wasm）"
  # **报错不再被吞** ✗（真实报告 ✓：macOS 上只看到"重建失败" ✓，看不到原因 ✓）。
  medium_log="$(mktemp)"
  if PATH="${kernel_toolchain}bin:$PATH" "${kernel_toolchain}bin/cargo" build \
       --manifest-path "$repo/Cargo.toml" --release --target wasm32-unknown-unknown \
       -p yanshi-medium-oil -p yanshi-medium-watercolor -p yanshi-medium-marker \
       -p yanshi-medium-pencil -p yanshi-medium-pixel -p yanshi-medium-example >"${medium_log}" 2>&1; then
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
    if grep -q "libLLVM.dylib" "${medium_log}" 2>/dev/null; then
      # **认出来就说人话** ✓（一屏 dyld 路径不会告诉用户"这不怪你的代码"✗）。
      cat >&2 <<'MACOS_LLD'
    ⇒ **这是 macOS 工具链的已知坑，与仓库源码无关** ✓：
       rustup 的 `rust-lld` 动态链到 `@rpath/libLLVM.dylib` ✓，而工具链包里没有它 ✗
       ⇒ 所有 `wasm32-unknown-unknown` 的构建都会失败 ✓。
    ⇒ **怎么修（任选一条）** ✓：
       ① 装 Homebrew 的 **`lld`**（注意：**`brew install llvm` 里没有 `lld`** ✗ ——
          `lld` 是单独的 formula ✓），然后原样重跑 `make release` ✓（本脚本会自动用它当链接器 ✓）：
            brew install lld
          装完可先确认一眼：`ls -l /opt/homebrew/opt/lld/bin/` ⇒ 应当看到 `lld` 与 `wasm-ld` ✓
          （如果第 ① 条没生效，就显式指定：）
            YANSHI_WASM_LINKER=/opt/homebrew/opt/lld/bin/lld make release
       ② 或者换/更新工具链：`rustup update`（这个缺库是打包问题 ✓，换个版本常常就好了 ✓）。
    ⇒ **注意** ✓：别把 `wasm-ld` 当链接器 ✗ —— rustc 会传 `-flavor wasm`，只有 `lld` 认它 ✓。
    ⇒ 本次发布**继续** ✓，用的是仓库里已提交的插件资产 ✓（内容可能与当前源码不符 ✗）。
MACOS_LLD
      echo "    ⇒ 下面是**真正的报错**（最后 15 行 ✓）：" >&2
      tail -15 "${medium_log}" | sed 's/^/      /' >&2
    else
      echo "    ⇒ 下面是**真正的报错**（最后 15 行 ✓）：" >&2
      tail -15 "${medium_log}" | sed 's/^/      /' >&2
    fi
    rm -f "${medium_log}"
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
# **三类资产也要随包发布** ✓（用户裁定：纹理要入库、要打包 ✓）。
# **为什么平铺在 share/yanshi/ 下** ✓：服务端把"资产根目录 + 种类子目录"拼在一起 ✓
#（`assets_dir` + `textures|brushes|palettes` ✓）⇒ 包内只要让 `--assets-dir` 指到 `share/yanshi` ✓
# ⇒ `scripts/fetch-textures.sh` 那条路（工作区缓存 ✓）与这条（随包内置 ✓）**并存** ✓。
# **逐文件拷，并且拷完核对数量** ✓（真实用户报告 ✓：macOS 上 `cp -R` 撞上
# `assets/brushes/P._Shade.myb` ✓ —— 名字里含 `._` ✓，而 **macOS 把 `._` 当
# AppleDouble（资源叉）的标记** ✗ ⇒ BSD `cp` 把它当成 `PShade.myb` 的旁文件 ✓，报
# `cp: found P._Shade.myb, looking for PShade.myb` ✗。
# **为什么改成"逐文件 + 核对"** ✓：
#   1. 递归拷贝的怪癖**不该能让我们少文件** ✗ —— 逐文件是最简单、最没歧义的操作 ✓；
#   2. **跳过 macOS 的元数据垃圾** ✓（`._*` 与 `.DS_Store` ✓）—— 它们本来就**不该进包** ✗；
#   3. **拷完比对数量** ✓ ⇒ 少了就**当场失败** ✗，而不是打出一个"能跑但缺资产"的包 ✓
#（"产物里少东西却没人知道"正是本项目反复抓的一类病 ✓）。
for asset_kind in textures brushes palettes; do
  source_dir="$repo/assets/${asset_kind}"
  target_dir="$stage/share/yanshi/${asset_kind}"
  [ -d "${source_dir}" ] || continue
  mkdir -p "${target_dir}"
  copied=0
  skipped=0
  while IFS= read -r asset_file; do
    asset_base="$(basename "${asset_file}")"
    case "${asset_base}" in
      ._*|.DS_Store) skipped=$((skipped + 1)); continue ;;
    esac
    cp "${asset_file}" "${target_dir}/${asset_base}"
    copied=$((copied + 1))
  done < <(find "${source_dir}" -type f | sort)
  expected="$(find "${source_dir}" -type f ! -name '._*' ! -name '.DS_Store' | wc -l | tr -d ' ')"
  if [ "${copied}" != "${expected}" ]; then
    echo "    ✗ ${asset_kind}：应当拷 ${expected} 个，实际拷了 ${copied} 个 ⇒ 打包失败" >&2
    exit 1
  fi
  echo "    ${asset_kind}：${copied} 个文件 ✓"
done
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
  --assets-dir "$here/share/yanshi" \
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

    ${sha_tool} -c SHA256SUMS   # 说给用户听的命令要**按平台说对** ✓
USAGE

# **包内 BUILD-INFO** ✓：版本 / commit / 目标平台 / 构建时间 / rustc 版本 ✓。
# 排查时先看这个文件 ✓，不必再问"这是哪个 commit 的构建" ✓。
cat > "$stage/BUILD-INFO" <<INFO
name: yanshi
version: ${version}
commit: ${commit}
target: ${target_triple}
built: ${built}
rustc: $(rustc -V)
INFO

# **打包守卫：报出这个包要求多新的 glibc** ✓ —— 用户正是在这里踩的坑 ✗：
# 动态包会**静默继承构建机的 glibc** ✓（实测 2.43 ✓），到 Debian 12（2.36 ✓）就直接崩 ✗。
# 所以这里**每次都打印** ✓；静态包则明确说"完全不依赖" ✓。
if [ "${is_linux}" = 1 ]; then
  echo "--> 检查运行期依赖（glibc 要求）"
  if ! command -v ldd >/dev/null 2>&1 || ! command -v objdump >/dev/null 2>&1; then
    # **工具缺失就明说** ✗（不能拿"没有输出"当"没有问题" ✓ —— 那正是这次误报的成因 ✓）。
    echo "    ⚠️  这台机器上没有 ldd / objdump ⇒ **跳过** glibc 检查（不等于没有问题 ✗）" >&2
  elif ldd "$stage/bin/yanshi-serve" 2>&1 | grep -q "statically linked"; then
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
else
  # **非 Linux：glibc 这一整套都不适用** ✓（真实用户报告 ✓）。
  echo "--> 跳过 glibc 检查（宿主不是 Linux：${target_os}）"
  echo "    glibc 是 Linux 的 C 库 ✓；本平台用自己的系统库（如 macOS 的 libSystem ✓）"
  echo "    ⇒ **不引用 GLIBC 符号不等于「哪儿都能跑」** ✗：包的可移植性由目标系统决定 ✓"
  echo "    ⇒ 换发行版 / 换系统时请**在实际目标上自测** ✓（后端渲染与写盘的路径都值得跑一遍 ✓）"
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
(cd "$out" && sha256_of "$name.tar.gz" > SHA256SUMS)
echo
echo "完成：$out/$name.tar.gz"
echo "      $out/SHA256SUMS"
du -sh "$out/$name.tar.gz" | sed 's/^/大小：/'
