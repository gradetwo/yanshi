#!/usr/bin/env bash
# 一键开发启动：构建 WASM 客户端内核（可选）+ 服务端（release），然后前台启动。
#
# 用法：
#   scripts/dev.sh                # 缺省 127.0.0.1:8110，数据落在 ~/.local/share/yanshi/workspace
#   scripts/dev.sh --port 9000    # 换端口
#   PORT=9000 ROOT=/tmp/yanshi scripts/dev.sh
#
# 设计取舍：**WASM 内核缺失不阻塞启动** —— 没有 wasm-bindgen 工具链时只打印一行提示，
# 服务端照样起来（查看器自动退化为服务端渲染）；这样"第一次跑起来"不需要装一堆东西。
set -euo pipefail
cd "$(dirname "$0")/.."

PORT="${PORT:-8110}"
ROOT="${ROOT:-$HOME/.local/share/yanshi/workspace}"
DOC="${DOC:-yanshi}"

# 解析 --port/--root（保持脚本简单，只支持这两个最常用的覆盖）。
while [ $# -gt 0 ]; do
  case "$1" in
    --port) PORT="$2"; shift 2 ;;
    --root) ROOT="$2"; shift 2 ;;
    --help|-h) sed -n '2,10p' "$0"; exit 0 ;;
    *) echo "未知参数：${1}（可用 --port/--root）" >&2; exit 2 ;;
  esac
done

# 端口预检：已经有同类实例在跑时不要报一堆构建/绑定错误，直接告诉用户地址。
# **⚠️ "有实例在跑"不等于"你看到的是这份代码"** ✗（真实踩过 ✓）：
# **查看器页面是编译进二进制的** ✓（`viewer.rs` 里是 `pub const PAGE` ✓，不是磁盘文件 ✓）
# ⇒ 一个**老进程**会一直吐**老界面** ✗，无论仓库里已经加了什么 ✓。
# 用户实测到的现象正是它 ✓：`make dev` 之后笔刷下拉里只有"内置画笔" ✓、调色板与纹理也没有 ✓ ——
# 因为那个端口上的进程是几小时前起的 ✓，而这里看到"健康"就**礼貌地退出**了 ✗。
# ⇒ 所以要比**版本身份** ✓：一致才让位 ✓，不一致就**说清并给出解法** ✓。
if curl -s -m 1 -o /dev/null "http://127.0.0.1:${PORT}/health"; then
  health_json="$(curl -s -m 1 "http://127.0.0.1:${PORT}/health" || true)"
  if printf '%s' "${health_json}" | grep -q '"ok":true'; then
    running_commit="$(printf '%s' "${health_json}" | sed -n 's/.*"commit":"\([^"]*\)".*/\1/p' | sed 's/-dirty$//')"
    checkout_commit="$(git rev-parse --short HEAD 2>/dev/null | sed 's/-dirty$//')"
    [ -n "${checkout_commit}" ] || checkout_commit="unknown"
    [ -n "${running_commit}" ] || running_commit="unknown"
    if [ "${running_commit}" = "${checkout_commit}" ]; then
      echo "已有 Yanshi 实例在 http://127.0.0.1:${PORT}/ 运行（身份 ${running_commit} ⇒ 与当前检出一致 ✓）"
      echo "  如需另起一个：make run PORT=9000"
      exit 0
    fi
    echo "⚠️  http://127.0.0.1:${PORT}/ 上**已经有一个实例**，但它跑的是**别的版本** ✗" >&2
    echo "     它自报 commit=${running_commit}，而当前检出是 ${checkout_commit} ⇒ **你看到的不是这份代码** ✗" >&2
    echo "     为什么这会让你以为新功能没做 ✗：**查看器页面是编译进二进制的**（不是磁盘文件）" >&2
    echo "     ⇒ 一个**老进程**会一直吐**老界面**，哪怕仓库里已经加了笔刷 / 调色板 / 纹理 ✓。" >&2
    echo "     两种解法（任选）：" >&2
    echo "       1) 停掉它再重跑：systemctl --user stop yanshi-serve" >&2
    echo "                        或 pkill -f 'yanshi-serve --bind 127.0.0.1:${PORT}'" >&2
    echo "                        然后重新 make dev" >&2
    echo "       2) 换个端口跑新的：make run PORT=9000" >&2
    echo "     注意：那个 systemd 用户服务是 **enabled** 的 ⇒ 每次开机都会再占住这个端口 ✓" >&2
    echo "     长期做法：让它指向你希望常驻的那份构建 ✓，或 systemctl --user disable yanshi-serve ✓" >&2
    exit 1
  fi
  echo "端口 ${PORT} 已被其它程序占用；换一个端口：make run PORT=9000" >&2
  exit 1
fi

echo "== 构建 WASM 客户端内核（可选）"
if cargo build -q -p yanshi-wasm --target wasm32-unknown-unknown --release 2>/dev/null; then
  if command -v wasm-bindgen >/dev/null 2>&1; then
    wasm-bindgen --target web --out-dir crates/yanshi-wasm/pkg --no-typescript \
      target/wasm32-unknown-unknown/release/yanshi_wasm.wasm
    echo "   ✓ crates/yanshi-wasm/pkg 已更新（浏览器端本地乐观渲染可用）"
  else
    echo "   ⚠️ 未安装 wasm-bindgen-cli，跳过 JS 胶水生成；"
    echo "      查看器仍可用（服务端渲染），需要本地乐观渲染时执行："
    echo "      cargo install wasm-bindgen-cli --version 0.2.129 --locked"
  fi
else
  echo "   ⚠️ 未安装 wasm32-unknown-unknown target，跳过；查看器将走服务端渲染。"
  echo "      需要时执行：rustup target add wasm32-unknown-unknown"
fi

echo "== 构建服务端（release）"
cargo build --release -p yanshi-http

mkdir -p "$ROOT"
echo "== 启动：http://127.0.0.1:$PORT/   （数据目录 ${ROOT}）"
exec target/release/yanshi-serve --bind "127.0.0.1:$PORT" --root "$ROOT" --doc "$DOC"
