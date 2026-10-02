# **偃师 Yanshi 的常用入口** ✓（真实用户建议 ✓："最好这个发布打包工作做成 make release 类似这样" ✓）。
#
# **为什么值得** ✓：打包脚本此前只能靠人记住路径与参数 ✓，
# 而**新克隆里直接跑它还会失败** ✗（缺 `crates/yanshi-wasm/pkg` ✓ —— 那是构建产物 ✓）。
# ⇒ 把常用动作收进 `make` ✓ ⇒ 一条 `make release` 就够 ✓，且**不需要记任何路径** ✓。

SHELL := /usr/bin/env bash
.DEFAULT_GOAL := help

# 便于 `make OUT=… release` ✓
OUT ?= dist

.PHONY: help release release-dynamic test test-heavy fmt clippy check dev clean

help:  ## 显示这份清单
	@echo "偃师 Yanshi —— 常用入口："
	@grep -E '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) | \
	  awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

release:  ## 打出发布包（默认静态，落在 dist/）
	scripts/package-release.sh --static --out $(OUT)

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

dev:  ## 开发用：构建 wasm 内核并起本地服务
	scripts/dev.sh

clean:  ## 清掉发布产物
	rm -rf dist
