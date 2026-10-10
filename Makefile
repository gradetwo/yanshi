# **偃师 Yanshi 的常用入口** ✓（真实用户建议 ✓："最好这个发布打包工作做成 make release 类似这样" ✓）。
#
# **为什么值得** ✓：打包脚本此前只能靠人记住路径与参数 ✓，
# 而**新克隆里直接跑它还会失败** ✗（缺 `crates/yanshi-wasm/pkg` ✓ —— 那是构建产物 ✓）。
# ⇒ 把常用动作收进 `make` ✓ ⇒ 一条 `make release` 就够 ✓，且**不需要记任何路径** ✓。

SHELL := /usr/bin/env bash
.DEFAULT_GOAL := help

# 便于 `make OUT=… release` ✓
OUT ?= dist

.PHONY: help release release-all release-cpu release-all-cpu release-kernel-gpu release-dynamic macos-benefit macos-benefit-selftest test test-heavy fmt clippy check dev clean

help:  ## 显示这份清单
	@echo "偃师 Yanshi —— 常用入口："
	@grep -E '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) | \
	  awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

release:  ## 打出发布包（缺省**只编当前平台**；可 TARGET=<triple> 指定）
	@if [ -n "$(TARGET)" ]; then \
	  echo "== 目标平台：$(TARGET)"; \
	  scripts/package-release.sh --target "$(TARGET)" --out $(OUT); \
	else \
	  scripts/package-release.sh --out $(OUT); \
	fi

release-all:  ## 逐个目标打包（没装的目标跳过并说明；YANSHI_RELEASE_TARGETS 可收紧清单）
	scripts/package-release.sh --all-targets --out $(OUT)

# **★ GPU 相关的两个目标 ✗ ★**（**第 490 轮 ✓）
#
# **∴ `release` 缺省就已经带 GPU**✗（**服务端 ✓）⇒ **∴ 这两个是**显式开关** ✓
#   ⇒ **∴ `release-cpu` ✗**：**纯 CPU 包**（**体积小 ✓；
#       **∴ 也用于**GPU 构建在某个目标上失败时** ✓）
#     ＋ **∴ `release-kernel-gpu` ✗**：**连浏览器内核也带 GPU** ✓
#       （**∴ 代价：内核依赖树 67 ⇒ 198 包 ✓）★**** ✓✓
release-cpu:  ## 打出纯 CPU 的发布包（不带 --features gpu；体积更小）
	scripts/package-release.sh --no-gpu \
	  $(if $(TARGET),--target "$(TARGET)",) --out $(OUT)

release-kernel-gpu:  ## 打出内核也带 GPU 的发布包（内核依赖树 67 ⇒ 198 包）
	scripts/package-release.sh --gpu --kernel-gpu \
	  $(if $(TARGET),--target "$(TARGET)",) --out $(OUT)

release-all-cpu:  ## 逐个目标打纯 CPU 包
	scripts/package-release.sh --all-targets --no-gpu --out $(OUT)

# **★ macOS 收益实测 ✗ ★**（**第 490 轮 ✓；**用户要求「增加在 macOS 平台测试收益的脚本」✓）
#
# **∴ 只能在 macOS 上跑 ✗**（**没有 `/proc` ⇒ 别的平台量不了 CPU 账 ✓）
#   ＋ **∴ 它**同时守三条硬约束**✗：**off⇒cpu／on 有解释／gpu⇒delta＝0 ✓
#     ＋ **∴ 且**在 macOS 上**适配器必须含 `metal`** ✓
#       （**∴ 若**是 `llvmpipe` 之类**软件渲染 ⇒ **直接判红** ✓ —— 第 318 轮的教训 ✓）★**** ✓✓
# **∴ 自检（**证明判据有牙 ✓）**：`make macos-benefit-selftest`
macos-benefit:  ## 在 macOS 上量 GPU 的两本账（需先构建带 gpu 的二进制）
	node scripts/tool-macos-gpu-benefit.mjs --binary target/release/yanshi-serve --rounds 3

macos-benefit-selftest:  ## 自检：证明 macOS 收益判据能红也能绿
	node scripts/tool-macos-gpu-benefit.mjs --self-test

targets:  ## 看看本机装了哪些发布目标（以及缺哪个要 rustup target add）
	scripts/package-release.sh --list-targets

release-dynamic:  ## 打出动态链接的发布包（仅在你确实需要时用）
	scripts/package-release.sh --dynamic --out $(OUT)

test:  ## 跑工作区测试（快的那一批）
	cargo test --workspace

test-heavy:  ## 跑长任务（--ignored；很慢，别和其他编译并行）
	cargo test --workspace --exclude yanshi-wasm --release -- --ignored

fmt:  ## 检查格式
	cargo fmt --all -- --check

clippy:  ## 严格 lint
	cargo clippy --workspace --all-targets -- -D warnings

check: fmt clippy test  ## 推送前的三项判据

serve:  ## 起一个"肯定是新的"服务端（先重编 -p yanshi-http ⇒ 用 curl 验工具前必跑）
	scripts/serve.sh --port $(PORT)

dev:  ## 开发用：构建 wasm 内核并起本地服务
	scripts/dev.sh

clean:  ## 清掉发布产物
	rm -rf dist
