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
    //
    // **⚠️ 这里原来只 watch 了 `.git/HEAD`，那是错的** ✗（真实用户报告 ✓）：
    // 在**普通分支检出**下 ✓ `.git/HEAD` 的内容是 `ref: refs/heads/main` ✓
    // —— **只是分支名，不含 commit 哈希** ✗ ⇒ **每次提交都不会改动这个文件** ✗
    // ⇒ cargo **永远认为本脚本是新鲜的** ✗ ⇒ 编出来的二进制里
    // `YANSHI_COMMIT` **冻在**"最后一次因别的原因重建"时的哈希 ✓。
    // **用户实测到的现象** ✓：包名是 `f8075f3` ✓ 而 `--version` 报 `dd2171d` ✓，
    // 两者差了几十个提交 ✗ ⇒ **他根本不知道自己在测哪一版** ✗——
    // 而这会**让每一次验收都可能验错对象** ✗，是最贵的一类缺陷 ✓。
    //
    // **修法** ✓：同时 watch **真正会变**的文件 ✓：
    // * `.git/refs/heads/<branch>` ✓ —— **提交时会被重写** ✓ ⇒ mtime 变 ✓；
    // * `.git/packed-refs` ✓ —— ref 被打包（`git gc`）后哈希写在这里 ✓；
    // * `.git/HEAD` ✓ —— 切分支 / 进 detached 状态时会变 ✓（worktree 常是这种 ✓）。
    // **worktree 的 `.git` 是一个文件** ✗（内容是 `gitdir: …` ✓）⇒ 上面的路径不存在 ✓
    // ⇒ cargo 对不存在的 watch 路径**不会报错** ✓，只是不起作用 ✓
    // ⇒ 所以**打包那层还要有自己的断言** ✓（见 `package-release.sh` ✓）。
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");
    if let Ok(head) = std::fs::read_to_string("../../.git/HEAD") {
        if let Some(reference) = head.trim().strip_prefix("ref: ") {
            // **分支引用文件才是提交时真正变的东西** ✓。
            println!("cargo:rerun-if-changed=../../.git/{reference}");
        }
        // 若 `.git/HEAD` 里直接是哈希 ✓（detached ✓）⇒ 上面那句 watch `.git/HEAD` 已经够了 ✓。
    }
    // **worktree / submodule 的情形** ✓：`.git` 是文件 ⇒ 顺着 `gitdir:` 再 watch 一次 ✓。
    if let Ok(gitfile) = std::fs::read_to_string("../../.git") {
        if let Some(dir) = gitfile.trim().strip_prefix("gitdir: ") {
            println!("cargo:rerun-if-changed=../../.git");
            println!("cargo:rerun-if-changed={dir}/HEAD");
            if let Ok(head) = std::fs::read_to_string(format!("{dir}/HEAD")) {
                if let Some(reference) = head.trim().strip_prefix("ref: ") {
                    println!("cargo:rerun-if-changed={dir}/{reference}");
                    println!("cargo:rerun-if-changed={dir}/packed-refs");
                }
            }
        }
    }
}
