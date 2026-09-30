# 常用入口：把「多条命令」收敛成一条。
#
#   make run     构建（含 WASM）+ 启动服务端   ← 最常用
#   make test    本地快速测试（秒级到两分钟）
#   make check   格式化 + lint
#   make smoke   WASM 运行时冒烟（抓"原生全绿、浏览器全崩"类回归）
#   make ui-check     真实浏览器 UI 回归（独立临时工作区，不污染日常数据）
#   make pixel-check  真实浏览器逐像素自检（同上）
#   make ci      本地能跑的全部检查（= check + test + smoke）
#   make mcp     启动 MCP stdio 服务（供 AI Agent 接入）
#
# 长任务（性能预算、10 万原子 fuzz、4K 剖面）**不在本地跑**，见 .github/workflows/heavy.yml：
#   gh workflow run heavy.yml && gh run list

PORT ?= 8110
ROOT ?= $(HOME)/.local/share/yanshi/workspace

.PHONY: help run build build-wasm build-medium test check smoke ui-check pixel-check medium-check ci mcp clean

help:
	@sed -n '2,14p' Makefile

run:
	@PORT=$(PORT) ROOT=$(ROOT) scripts/dev.sh

build:
	cargo build --release -p yanshi-http

build-wasm:
	cargo build -p yanshi-wasm --target wasm32-unknown-unknown --release
	wasm-bindgen --target web --out-dir crates/yanshi-wasm/pkg --no-typescript \
	  target/wasm32-unknown-unknown/release/yanshi_wasm.wasm

test:
	cargo test --workspace

check:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

smoke:
	scripts/wasm-smoke.sh

# 真实浏览器检查：在**独立临时工作区**里起服务端，避免把渲染孤儿写进日常工作区。
ui-check:
	scripts/with-temp-server.sh node scripts/browser-ui-check.mjs

build-medium:
	@cargo build -p yanshi-medium-example --target wasm32-unknown-unknown --release
	@cp target/wasm32-unknown-unknown/release/yanshi_medium_example.wasm assets/mediums/example-dab.wasm
	@echo "  ✓ 示范介质插件已构建到 assets/mediums/example-dab.wasm"

medium-check:
	@node scripts/medium-abi-check.mjs

pixel-check:
	scripts/with-temp-server.sh node scripts/browser-pixel-check.mjs

ci: check test smoke

mcp:
	cargo run --release -p yanshi-mcp -- --doc $(ROOT)

clean:
	cargo clean
	rm -rf crates/yanshi-wasm/pkg
