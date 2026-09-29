# 贡献指南

感谢参与偃师（Yanshi）。本文说明开发环境、提交前检查与协作流程。设计语义以 [docs/design/yanshi-v1.0-draft4.md](docs/design/yanshi-v1.0-draft4.md) 为唯一权威。

## 开发环境

- **Rust stable 1.85 或更高版本**（`rustup toolchain install stable`）。
- 组件：`rustfmt` 与 `clippy`（`rustup component add rustfmt clippy`）。
- 本仓库为 Cargo workspace。请在仓库根目录执行所有命令。
- `Cargo.lock` 纳入版本控制（仓库包含二进制目标），修改依赖时请一并提交 `Cargo.lock` 变更。

## 提交前检查

提交或发起 PR 前，以下三条命令必须全部通过，与 CI 一致：

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

涉及模糊测试的改动，另需在本地运行长时用例：

```bash
cargo test --workspace --release -- --ignored
```

说明：

- `cargo fmt --all` 后工作区不得有未格式化的改动；CI 使用 `cargo fmt --all -- --check`。
- clippy 警告视为错误，不允许用 `#[allow]` 掩盖问题，除非在同行注明原因。
- 不提交注释掉的死代码、调试打印或临时文件。

## 提交信息规范

- 遵循 [Conventional Commits](https://www.conventionalcommits.org/)：`<type>(<scope>): <subject>`。
- 常用 `type`：`feat`、`fix`、`docs`、`test`、`refactor`、`perf`、`chore`、`ci`。
- `scope` 建议使用 crate 或模块名，例如 `core`、`fold`、`blob`、`ci`、`docs`。
- 正文可使用**中文或英文**，同一提交内保持一致即可。
- 提交信息应说明“为什么改”，而不只是“改了什么”。
- 示例：
  - `feat(fold): 实现 revert 级联失效与警告记录`
  - `test(blob): 增加 GC 不破坏历史可重放性的属性测试`

## PR 要求

- 一个 PR 解决一件事，避免混杂无关重构。
- **新增或修改折叠语义、GC 语义时，必须附带属性测试**（proptest 或等价方案），覆盖第 5.3 节折叠代数不变量：幂等性、收敛性、无孤儿引用、revert-reapply 往返、历史可重放。
- 涉及 GC 的改动必须证明不破坏可回放性：GC 根集 = 全日志引用闭包，历史 blob 不得被清理。
- 涉及 Blob 的改动需覆盖提交顺序协议（blob 先行，原子提交时校验引用存在）。
- 修复缺陷时请附回归测试；新增公开 API 需有文档注释与示例。
- PR 描述中说明动机、方案、测试方式与已知限制。
- CI 必须全绿（fmt、clippy、stable/beta 测试矩阵、长时 fuzz）。

## 设计文档变更流程

设计文档是冻结的规范，代码是其翻译。流程为**先改设计文档，再改代码**：

1. 在 `docs/design/` 下更新设计文档（当前冻结修订为 `yanshi-v1.0-draft4.md`）。
2. 说明变更动机、对既有语义的影响，以及受影响的不变量与测试。
3. 在同一 PR 中同步实现、测试与文档，不允许代码先合入、文档后补。
4. 任何语义变更都不得让代码与设计文档静默分叉。若实现中发现文档有误，先在文档中修正，再改代码，并在 PR 中显式指出该修正。
5. 实现级问题（笔误、表述歧义、缺失的边界说明）可直接在 PR 中随附修正；改变协议的语义变更需要独立评审。

## 许可证

贡献即表示同意以 [MIT 许可证](LICENSE) 授权你的贡献。

Copyright (c) 2026 The Yanshi Authors
