//! **把"这一版是哪一版"编进二进制** ✓ —— 用户提的排查建议 ✓。
//!
//! **为什么值得** ✓：拿到一个 release 包或一个正在跑的服务 ✓，第一句话往往是
//! "**这是哪个 commit 的构建**" ✓。把它编进去 ✓，就不用再问、也不用再猜 ✓。
//!
//! **零依赖做法** ✓：`Command::new("git")` 取短 hash ✓（取不到就写 `unknown` ✓，**不失败** ✗）、
//! `date -u` 取 UTC 时间 ✓（同样有退回 ✓）⇒ 没有 git 的环境里照样能编 ✓。
//!
//! **`-dirty` 后缀** ✓：工作区有未提交改动时标出来 ✓ —— 测试时**最怕**"跑的不是那版代码" ✗，
//! 这一后缀把那种情况**摆在明面上** ✓。

use std::process::Command;

fn run(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn main() {
    // **commit** ✓：短 hash + 脏标记 ✓
    let commit = match run("git", &["rev-parse", "--short", "HEAD"]) {
        Some(hash) => {
            let dirty = run("git", &["status", "--porcelain"])
                .map(|text| !text.is_empty())
                .unwrap_or(false);
            if dirty {
                format!("{hash}-dirty")
            } else {
                hash
            }
        }
        None => "unknown".to_owned(),
    };
    // **构建时间** ✓（UTC ✓）
    let built = run("date", &["-u", "+%Y-%m-%d %H:%M UTC"]).unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=YANSHI_COMMIT={commit}");
    println!("cargo:rustc-env=YANSHI_BUILD_TIME={built}");
    // **commit 变了要重编** ✓（否则改了代码却还报旧 hash ✗）。
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}
