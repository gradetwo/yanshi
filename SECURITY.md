# Security Policy / 安全策略

## Reporting a vulnerability

Please report security issues **privately**, not in a public issue or pull request:

- Email: **<yanshi@wangda.today>**
- Or use GitHub's [private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability) on this repository.

Please include: affected version/commit, a minimal reproduction, the impact you believe it has, and
whether you plan to disclose it publicly. We aim to acknowledge within 5 working days and to ship a fix
or a mitigation plan before any public disclosure.

## Scope

Yanshi runs as a headless service plus a browser client. Reports we care about most:

- capability token bypass or privilege escalation on the HTTP/WebSocket transport (`crates/yanshi-http`)
- object/blob store escapes: a document reading or overwriting blobs it does not own (`crates/yanshi-server`, `crates/yanshi-core`)
- denial of service through unbounded frames, atoms, jobs or tile budgets
- arbitrary file access through the tool layer or the brand/WASM asset allowlists
- memory-safety issues in the compute kernel (Rust, but parsing untrusted JSON)

## 安全漏洞上报（中文）

请**不要**开公开 issue，直接发邮件到 <yanshi@wangda.today>，或使用 GitHub 的私有漏洞上报功能。
邮件里请写明：受影响的版本/提交、最小复现、你认为的影响范围、以及是否计划公开披露。
我们会在 5 个工作日内确认，并在公开披露前给出修复或缓解方案。

## Supported versions

The project is pre-1.0; only the `main` branch receives security fixes.
