//! 脚本可移植性守卫。
//!
//! 起因：`scripts/dev.sh` 里写了 `"…数据目录 $ROOT）"` —— 全角右括号紧跟变量名。
//! Linux 的 bash 5 容忍这种写法，但 **macOS 自带的 bash 3.2** 会把多字节字符的首字节
//! 当作变量名的一部分，于是报 `ROOT<乱码>: unbound variable` 而启动失败。
//!
//! 因此这里做两条检查（都在普通测试套件里跑，CI 的快速作业会执行）：
//! 1. `scripts/*.sh` 与 `Makefile` 中，`$VAR` 之后**不得紧跟非 ASCII 字符** —— 必须写 `${VAR}`；
//! 2. 每个脚本必须能通过 `bash -n`（语法检查），且带 shebang。

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate 应位于 <repo>/crates/yanshi-server")
        .to_path_buf()
}

fn scripts() -> Vec<PathBuf> {
    let dir = repo_root().join("scripts");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("读取 {dir:?} 失败：{error}"))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "sh"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "scripts/ 下应至少有一个脚本");
    files
}

/// 提取 `$VAR` / `${VAR}` 中的变量引用及其在行内的结束位置。
fn variable_references(line: &str) -> Vec<(String, usize)> {
    let bytes: Vec<char> = line.chars().collect();
    let mut found = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == '$' {
            if index + 1 < bytes.len() && bytes[index + 1] == '{' {
                // ${VAR}：跳到右花括号。
                if let Some(close) = bytes[index + 2..].iter().position(|c| *c == '}') {
                    let name: String = bytes[index + 2..index + 2 + close].iter().collect();
                    found.push((format!("${{{name}}}"), index + 2 + close + 1));
                    index += 2 + close + 1;
                    continue;
                }
            } else {
                let name: String = bytes[index + 1..]
                    .iter()
                    .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
                    .collect();
                if !name.is_empty() {
                    let end = index + 1 + name.len();
                    found.push((format!("${name}"), end));
                    index = end;
                    continue;
                }
            }
        }
        index += 1;
    }
    found
}

#[test]
fn dollar_variables_never_touch_non_ascii_text() {
    let root = repo_root();
    let mut offenders = Vec::new();
    let mut targets = scripts();
    targets.push(root.join("Makefile"));
    for path in targets {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (lineno, line) in text.lines().enumerate() {
            for (reference, end) in variable_references(line) {
                // 只看未加花括号的引用；`${VAR}` 本身已安全。
                if reference.starts_with("${") {
                    continue;
                }
                let follow: Vec<char> = line.chars().collect();
                if let Some(next) = follow.get(end) {
                    if !next.is_ascii() {
                        offenders.push(format!(
                            "{}:{}: `{reference}` 紧跟非 ASCII 字符 {next:?}；\
                             请写成 ${{{}}}（macOS 的 bash 3.2 会把它算进变量名）",
                            path.strip_prefix(&root).unwrap_or(&path).display(),
                            lineno + 1,
                            reference.trim_start_matches('$')
                        ));
                    }
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "脚本存在可移植性问题（共 {} 处）：\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

#[test]
fn scripts_parse_and_have_shebangs() {
    for path in scripts() {
        let text = std::fs::read_to_string(&path).expect("脚本应可读");
        assert!(text.starts_with("#!"), "{} 缺少 shebang", path.display());
        let output = std::process::Command::new("bash")
            .arg("-n")
            .arg(&path)
            .output()
            .unwrap_or_else(|error| panic!("无法执行 bash -n：{error}"));
        assert!(
            output.status.success(),
            "{} 未通过 bash -n：{}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
