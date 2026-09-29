# 常用入口：把「多条命令」收敛成一条。
#
#   make run     构建（含 WASM）+ 启动服务端   ← 最常用
#   make test    本地快速测试（秒级到两分钟）
#   make check   格式化 + lint
#   make smoke   WASM 运行时冒烟（抓"原生全绿、浏览器全崩"类回归）
#   make ci      本地能跑的全部检查（= check + test + smoke）
#   make mcp     启动 MCP stdio 服务（供 AI Agent 接入）
#
# 长任务（性能预算、10 万原子 fuzz、4K 剖面）**不在本地跑**，见 .github/workflows/heavy.yml：
#   gh workflow run heavy.yml && gh run list

PORT ?= 8110
ROOT ?= $(HOME)/.local/share/yanshi/workspace

.PHONY: help run build build-wasm test check smoke ci mcp clean

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

ci: check test smoke

mcp:
	cargo run --release -p yanshi-mcp -- --doc $(ROOT)

clean:
	cargo clean
	rm -rf crates/yanshi-wasm/pkg
