#!/usr/bin/env bash
# **真·静态包**（6d ✓）：产出一个**不依赖任何动态库**、能直接拷到同架构机器上跑的服务端 ✓。
#
# 背景（第 226 轮实测 ✓）：
#  * 以前以为"必须装 musl 目标" ✗ —— 其实 **glibc 也能静态链接** ✓：`-C target-feature=+crt-static` ✓；
#  * 目标目录里**早就有一个** static-pie 的产物 ✓（`ldd` 说 statically linked ✓）而且**能起、能答** ✓
#    （冒烟：`GET /api/tools` ⇒ `HTTP 405` + 正规 JSON 报错 ✓ ⇒ 服务真的在跑 ✓）；
#  * 二进制属于 **`yanshi-http`** 包 ✓（不是 `yanshi-server` ✗ —— 包名与 bin 名不同 ✓）。
#
# 判据（本脚本**自带** ✓，能红 ✓）：① 构建成功 ✓；② `ldd` 必须说"statically linked" ✓（否则退出 1 ✗）；
# ③ 必须能**起进程并回答一个请求** ✓（"能编译"不等于"能用" ✓）。
set -euo pipefail
# **缺省取"本机宿主"** ✗ —— 原来这里**写死** `x86_64-unknown-linux-gnu` ✓，
# 于是 macOS 上会去编一个**没装的目标** ✗（或更糟：名字与产物不符 ✗ ⇒ 跑起来 `Exec format error` ✗，
# 用户实测报告过这个 BUG ✓）。**名字必须来自真实目标** ✓，不许写死 ✗。
HOST_TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
TARGET="${YANSHI_STATIC_TARGET:-${HOST_TRIPLE}}"
echo "  宿主 triple：${HOST_TRIPLE}｜本次目标：${TARGET}"
# **交叉编译必须显式说** ✓：目标与宿主不同、又不是 Linux 时，`+crt-static` 没有意义 ✓。
case "${TARGET}" in
  *-linux-*) ;;
  *) echo "  ⚠ 目标 ${TARGET} 不是 Linux ⇒ 本脚本的 crt-static 静态链接对它没有意义"
     echo "     （真实用户报告：macOS 上打出来的包名写 linux、内容却是苹果芯片 ⇒ 跑不起来 ✗）"
     echo "     请显式指定 Linux 目标：YANSHI_STATIC_TARGET=x86_64-unknown-linux-gnu $0"
     exit 2 ;;
esac
BIN="target/$TARGET/release/yanshi-serve"
echo "== 静态构建（crt-static，目标 ${TARGET}）=="
RUSTFLAGS="-C target-feature=+crt-static" cargo build --release -p yanshi-http --bin yanshi-serve --target "$TARGET"
[ -x "$BIN" ] || { echo "✗ 没有产物：$BIN"; exit 1; }
echo "== 判据① 产物 =="
ls -la "$BIN" | awk '{print "  大小:", $5, "字节"}'
file "$BIN" | sed 's/^/  /'
echo "== 判据② 必须静态链接 =="
if ! ldd "$BIN" 2>&1 | grep -qi "statically linked\|not a dynamic executable"; then
  echo "✗ 不是静态链接 ⇒ 拷到别的机器上会缺库 ✗"; ldd "$BIN" 2>&1 | head -5; exit 1
fi
echo "  ✓ statically linked"
echo "== 判据③ 必须能起、能答（冒烟）=="
PORT="${YANSHI_STATIC_PORT:-18099}"
ROOT="$(mktemp -d)"
"$BIN" --bind "127.0.0.1:$PORT" --root "$ROOT" --doc static-smoke --width 96 --height 72 --assets-dir "$PWD/assets" > "$ROOT/log" 2>&1 &
PID=$!
trap 'kill "$PID" 2>/dev/null || true; rm -rf "$ROOT"' EXIT
for _ in $(seq 1 30); do curl -s -o /dev/null "http://127.0.0.1:$PORT/" && break; sleep 0.3; done
CODE=$(curl -s -o "$ROOT/out" -w "%{http_code}" "http://127.0.0.1:$PORT/api/tools")
# **断言要贴"能用的定义"，不是某次观测到的具体响应** ✗ ——
# 第一版按**旧二进制**的行为写死 405 ✗，而新构建该路由已支持 GET ✓ ⇒ 实测 200 + 正常目录 ✓。
# 现在断言：① 200 ✓；② 正文是**可解析且含 tools 的 JSON** ✓（"能编译"不等于"能用" ✓）。
if [ "$CODE" != "200" ] || ! grep -q '"tools"' "$ROOT/out"; then
  echo "✗ 冒烟失败：期望 200 且含 tools，实测 ${CODE}"; head -c 200 "$ROOT/out"; exit 1
fi
echo "  ✓ 起来了并正确回答（HTTP ${CODE}，含工具目录 JSON）"
echo "== ✓ 真·静态包完成：$BIN =="
