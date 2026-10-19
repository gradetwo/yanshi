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

/// **`scripts/*.sh` 与 `Makefile` 不许用 bash 4+ 独有特性**（第 1018 轮新加）
///
/// **∴ 起因**：第 1017 轮我发现 `watch-ci.sh` 里写了 `declare -A` ✗
///   ⇒ 那是 **bash 4+** 的关联数组 ⇒ macOS 自带的 bash **3.2** 会直接报
///     `declare: -A: invalid option` ⇒ 脚本**跑不起来** ✗
///   ⇒ 而这条测试**当时只查**「`$VAR` 紧跟非 ASCII」⇒ **抓不到它** ✗
///
/// **∴ 所以**补上这张表：凡是 macOS bash 3.2 不支持／行为不同的写法，一律禁止 ✓
#[test]
fn scripts_avoid_bash4_only_features() {
    // (子串, 人话) —— 用**纯子串**匹配（不引 regex 依赖）
    let forbidden: &[(&str, &str)] = &[
        (
            "declare -A",
            "`declare -A` 关联数组（bash 4+；改用临时文件）",
        ),
        ("mapfile ", "`mapfile`（bash 4+；改用 while read）"),
        ("readarray ", "`readarray`（bash 4+；改用 while read）"),
        ("declare -n", "`declare -n` nameref（bash 4.3+）"),
        ("grep -P", "`grep -P`（macOS 的 BSD grep 不支持）"),
        ("grep -oP", "`grep -oP`（同上）"),
        ("sed -i ", "`sed -i` 无后缀（macOS 要求 `sed -i ''`）"),
        ("^^}", "${VAR^^} 大小写转换（bash 4+）"),
        (",,}", "${VAR,,} 大小写转换（bash 4+）"),
    ];
    let mut violations = Vec::new();
    let mut files = scripts();
    let makefile = repo_root().join("Makefile");
    if makefile.is_file() {
        files.push(makefile);
    }
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (lineno, line) in text.lines().enumerate() {
            // 跳过注释行（注释里提到这些词是**说明**，不是用法）
            if line.trim_start().starts_with('#') {
                continue;
            }
            for (needle, what) in forbidden {
                if line.contains(needle) {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    violations.push(format!("{name}:{}: {what}", lineno + 1));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "脚本用了 macOS bash 3.2 不支持的写法（共 {} 处）：\n  {}",
        violations.len(),
        violations.join("\n  ")
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
