//! **脚本卫生** ✓：`scripts/*.sh` 里不允许出现"**被引用却从未赋值**"的变量 ✗。
//!
//! **为什么值得一条测试** ✓（真实翻车 ✓）：我把 `host_os` 改名成 `target_triple` 时**漏改了一处引用** ✗
//! ⇒ 脚本带着 `set -u` ✓ ⇒ 用户在自己的 **macOS** 上跑 `make release` ⇒
//! `scripts/package-release.sh: line 447: host_os: unbound variable` ✗ ✓ ——
//! **本机 Linux 上跑不到那一行**（那是"非 Linux"分支 ✓）⇒ **本地全绿也拦不住** ✗ ✓。
//! 这与"大小写冲突在 Linux 上看不出来"是同一种病 ✓ ⇒ **只有扫一遍才能挡住** ✓。
//!
//! **判定规则** ✓（刻意保守 ✓，宁可漏报、也不要误报 ✓）：
//! * 只看**小写**变量名 ✓ —— 全大写按约定当**环境变量** ✓（`YANSHI_*` / `PORT` / `HOME`…… ✓）；
//! * `local x` / `x=` / `x+=` / `for x in` / `read [-r] x` / `x=$(…)` 都算"赋值" ✓；
//! * 函数名、位置参数（`$1` ✓）、特殊变量（`$?` `$@` `$#` `$$` `$!` `$-` `$*` ✓）跳过 ✓。

use std::collections::BTreeSet;
use std::path::Path;

/// 特殊变量与 shell 内置 ✓（出现在脚本里是正常的 ✓）。
const SPECIAL: [&str; 10] = ["?", "@", "#", "$", "!", "-", "*", "0", "1", "2"];

fn collect_scripts(dir: &Path, found: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_scripts(&path, found);
        } else if path.extension().map(|ext| ext == "sh").unwrap_or(false) {
            found.push(path);
        }
    }
}

/// **是不是一个小写标识符** ✓（`foo` / `foo_bar` ✓；`Foo` / `FOO` 不算 ✓）。
fn is_lower_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .next()
            .map(|c| c.is_ascii_lowercase())
            .unwrap_or(false)
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[test]
fn every_script_variable_is_assigned_somewhere() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut scripts = Vec::new();
    collect_scripts(&root.join("scripts"), &mut scripts);
    assert!(
        scripts.len() >= 3,
        "至少要扫到几个脚本，实际 {}",
        scripts.len()
    );

    let mut problems = Vec::new();
    for script in &scripts {
        let text = std::fs::read_to_string(script).expect("脚本应当是 UTF-8");
        // **先剥掉注释** ✓ —— 注释里出现变量名是正常的 ✓（这次那两处就在注释里 ✓）。
        let code: String = text
            .lines()
            .map(|line| match line.find('#') {
                Some(at) => &line[..at],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n");
        // **收集赋值** ✓。
        let mut assigned: BTreeSet<String> = BTreeSet::new();
        for token in code.split(|c: char| {
            !(c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '{' || c == '}')
        }) {
            // `x=…` / `x+=…` 这两种形式：上面那个切分会把 `x=` 切成 `x` 与空 ✓
            // ⇒ 换个更直接的办法：在原文里找 `name=` 与 `name+=` ✓（见下 ✓）。
            let _ = token;
        }
        for (index, _) in code.match_indices('=') {
            // 往左取标识符 ✓
            let before = &code[..index];
            let name: String = before
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let name = name.trim_start_matches(['+', '-', ':']).to_string();
            if is_lower_name(&name) {
                assigned.insert(name);
            }
        }
        // `for x in` / `read x` / `local x` / `declare x` / `export x` ✓
        for keyword in [
            "for ", "read ", "read -r ", "local ", "declare ", "export ", "unset ",
        ] {
            for (index, _) in code.match_indices(keyword) {
                let rest = &code[index + keyword.len()..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if is_lower_name(&name) {
                    assigned.insert(name);
                }
            }
        }
        // `while IFS= read -r x` 那种：`read -r` 后面也可能带别的参数 ✓ ⇒ 再兜一层 ✓。
        for (index, _) in code.match_indices("read ") {
            let rest = &code[index + 5..];
            for word in rest.split_whitespace().take(3) {
                let word = word.trim_end_matches(';');
                if is_lower_name(word) {
                    assigned.insert(word.to_string());
                }
            }
        }

        // **收集引用** ✓。
        let bytes = code.as_bytes();
        let mut index = 0usize;
        while index < bytes.len() {
            if bytes[index] != b'$' {
                index += 1;
                continue;
            }
            index += 1;
            if index >= bytes.len() {
                break;
            }
            let braced = bytes[index] == b'{';
            if braced {
                index += 1;
            }
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            let name = &code[start..index];
            if braced && index < bytes.len() && bytes[index] == b'}' {
                index += 1;
            }
            if name.is_empty() || !is_lower_name(name) || SPECIAL.contains(&name) {
                continue;
            }
            if !assigned.contains(name) {
                problems.push(format!(
                    "{}：${{{name}}} 被引用但从未赋值",
                    script.display()
                ));
            }
        }
    }
    problems.sort();
    problems.dedup();
    assert!(
        problems.is_empty(),
        "脚本里出现了「被引用却从未赋值」的变量 ⇒ 在 `set -u` 下会当场炸 ✗ \
         （`host_os` 就是这么把 macOS 用户的 `make release` 弄挂的 ✓）：\n{}",
        problems.join("\n")
    );
}
