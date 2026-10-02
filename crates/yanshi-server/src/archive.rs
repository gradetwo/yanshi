//! **工程包：零依赖的 tar 写入** ✓（真实用户提的缺口 ✓："缺一键把 `atoms.jsonl`、`meta.json`
//! 与 CAS blobs 归档为 `.yanshi` 工程文件的内置命令" ✓）。
//!
//! **为什么是 tar 而不是 zip** ✓：zip 需要 **deflate** ✗（要么引依赖 ✓ 要么手写压缩 ✓），
//! 而本项目的硬约束是**零外部依赖** ✓。tar **只是一层 512 字节头 + 原样数据** ✓
//! ⇒ 手写 60 行 ✓、**任何系统都能 `tar -xf` 打开** ✓、内容还能直接 `tar -xOf` 看 ✓ ✓。
//! **代价** ✗：不压缩 ✓（工程包主体是 PNG/原始像素 ✓，本来就压不动 ✓）。
//!
//! **格式细节（ustar ✓，都按 POSIX 来 ✓）**：
//! 头部 512 字节 ✓ —— `name[100] mode[8] uid[8] gid[8] size[12] mtime[12] chksum[8] typeflag[1]
//! linkname[100] magic[6] version[2] uname[32] gname[32] devmajor[8] devminor[8] prefix[155] pad[12]` ✓；
//! 数字用**八进制 ASCII**、以 `NUL` 结尾 ✓；校验和 = 把 `chksum` 字段当**空格**时整头的字节和 ✓；
//! 数据后面补到 512 的整数倍 ✓；结尾是**两个全零块** ✓。

/// 一个待写入的文件 ✓（路径 + 内容 ✓）。
pub struct TarEntry {
    /// 包内路径 ✓（用 `/` 分隔 ✓，不含前导 `/` ✓）。
    pub path: String,
    /// 文件内容 ✓。
    pub bytes: Vec<u8>,
}

/// 把一个字节串写成 tar 的八进制字段 ✓（宽度含结尾的 `NUL` ✓）。
fn octal(value: u64, width: usize) -> Vec<u8> {
    // 宽度 `n` 的字段：`n-1` 位八进制 + 一个 `NUL` ✓（POSIX 就这么规定的 ✓）。
    let text = format!("{:0>width$o}", value, width = width - 1);
    let mut out = text.into_bytes();
    out.truncate(width - 1);
    out.push(0);
    out
}

/// **把若干文件打成一个 tar** ✓（纯内存 ✓，调用方自己落盘 ✓）。
pub fn write_tar(entries: &[TarEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    for entry in entries {
        let mut header = [0u8; 512];
        // ① 路径 ✓（ustar 的单字段上限 100 ✓；超长的走 `prefix` 拆分 ✓）
        let (prefix, name) = split_path(&entry.path);
        let name_bytes = name.as_bytes();
        header[..name_bytes.len().min(100)]
            .copy_from_slice(&name_bytes[..name_bytes.len().min(100)]);
        if let Some(prefix) = prefix {
            let prefix_bytes = prefix.as_bytes();
            header[345..345 + prefix_bytes.len().min(155)]
                .copy_from_slice(&prefix_bytes[..prefix_bytes.len().min(155)]);
        }
        // ② 权限/属主/大小/时间 ✓
        header[100..108].copy_from_slice(&octal(0o644, 8)); // mode
        header[108..116].copy_from_slice(&octal(0, 8)); // uid
        header[116..124].copy_from_slice(&octal(0, 8)); // gid
        header[124..136].copy_from_slice(&octal(entry.bytes.len() as u64, 12)); // size
        header[136..148].copy_from_slice(&octal(0, 12)); // mtime（**写 0** ✓ ⇒ 同一份内容打出的包**逐字节一致** ✓）
                                                         // ③ 普通文件 ✓ + ustar 魔数 ✓
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        header[265..269].copy_from_slice(b"root");
        header[297..301].copy_from_slice(b"root");
        // ④ 校验和 ✓：先按空格算 ✓，再写进去 ✓
        for slot in header[148..156].iter_mut() {
            *slot = b' ';
        }
        let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
        header[148..156].copy_from_slice(&octal(checksum, 8));
        out.extend_from_slice(&header);
        // ⑤ 数据 + 补齐 ✓
        out.extend_from_slice(&entry.bytes);
        let padding = (512 - entry.bytes.len() % 512) % 512;
        out.extend(std::iter::repeat_n(0u8, padding));
    }
    // ⑥ 结束：两个全零块 ✓
    out.extend(std::iter::repeat_n(0u8, 1024));
    out
}

/// ustar 的路径拆分 ✓：`prefix`（≤155 ✓）+ `name`（≤100 ✓），在 `/` 处切 ✓。
fn split_path(path: &str) -> (Option<String>, String) {
    if path.len() <= 100 {
        return (None, path.to_owned());
    }
    // 从右往左找第一个能让 `name` 不超过 100 字节的 `/` ✓。
    let bytes = path.as_bytes();
    let mut cut = None;
    for index in (0..bytes.len()).rev() {
        if bytes[index] == b'/' && path.len() - index - 1 <= 100 && index <= 155 {
            cut = Some(index);
            break;
        }
    }
    match cut {
        Some(index) => (Some(path[..index].to_owned()), path[index + 1..].to_owned()),
        // 实在切不开 ✓（单段超长 ✓）：截断到 100 ✓ —— **宁可截断也不写坏包** ✓。
        None => (None, path.chars().take(100).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **写出来的 tar 必须能被系统 `tar` 打开** ✓ —— 这是"格式对不对"的唯一真判据 ✓。
    ///
    /// **为什么不在 Rust 里自己解析一遍** ✗：那只是**自证** ✓ —— 我按自己的理解写 ✓ 再按自己的理解读 ✓
    /// 两边一起错也测不出来 ✓。**用系统的 `tar`** ✓（GNU tar ✓）才是**独立**的判据 ✓。
    #[test]
    fn the_tar_we_write_is_readable_by_system_tar() {
        let dir = std::env::temp_dir().join(format!("yanshi_tar_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("临时目录");
        let entries = vec![
            TarEntry {
                path: "meta.json".to_owned(),
                bytes: br#"{"doc_id":"d1"}"#.to_vec(),
            },
            TarEntry {
                path: "blobs/sha256/ab/cd/abcd".to_owned(),
                bytes: vec![7u8; 1000],
            },
            TarEntry {
                path: "render.png".to_owned(),
                bytes: vec![137, 80, 78, 71],
            },
        ];
        let tar = write_tar(&entries);
        let path = dir.join("pack.yanshi");
        std::fs::write(&path, &tar).expect("写包");
        // ① **系统 tar 能列出内容** ✓
        let listed = std::process::Command::new("tar")
            .args(["-tf", path.to_str().unwrap()])
            .output()
            .expect("系统应当有 tar");
        assert!(
            listed.status.success(),
            "tar -tf 应当成功：{:?}",
            String::from_utf8_lossy(&listed.stderr)
        );
        let names = String::from_utf8_lossy(&listed.stdout);
        for expected in ["meta.json", "blobs/sha256/ab/cd/abcd", "render.png"] {
            assert!(names.contains(expected), "tar 里应当有 {expected}：{names}");
        }
        // ② **逐个解出来内容要一模一样** ✓（含补齐边界：1000 字节不是 512 的倍数 ✓）
        for entry in &entries {
            let extracted = std::process::Command::new("tar")
                .args(["-xOf", path.to_str().unwrap(), &entry.path])
                .output()
                .expect("tar 应当能解出");
            assert!(extracted.status.success(), "解 {} 应当成功", entry.path);
            assert_eq!(
                extracted.stdout, entry.bytes,
                "{} 的内容应当逐字节一致",
                entry.path
            );
        }
        // ③ **同一份内容 ⇒ 包逐字节一致** ✓（mtime 写 0 ✓ ⇒ 打两次不出两个不同的包 ✓）
        assert_eq!(write_tar(&entries), tar, "同一个输入应当打出同一个包");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **超过 100 字节的路径要走 ustar 的 `prefix`** ✓（CAS 的路径是 `blobs/sha256/ab/cd/<64 位 hex>` ✓）。
    #[test]
    fn long_paths_survive_the_ustar_prefix_split() {
        // **注意** ✓：真实的 CAS 路径是 `blobs/sha256/ab/cd/<64 位 hex>` = **84 字节** ✓
        // ⇒ **它其实没超** ✗（我第一版想当然地断言"本来就该超过 100" ✓ ⇒ 红了 ✓，
        //   而红的原因是**我的假设错** ✓，不是代码错 ✓）。前缀拆分因此是**防御性**的 ✓ ——
        // 但"文件系统里可能出现多深的目录"不该由我猜 ✓ ⇒ 用一个**真的**超长路径测它 ✓。
        // **按长度构造** ✓，不再"看着差不多就断言" ✗（我已经在这上面连栽两次 ✓ ——
        // 第一次 84 字节 ✓、第二次 96 字节 ✓，两次都是**我的估计**错 ✓）。
        let mut long = String::from("blobs/sha256/ab/cd");
        while long.len() < 130 {
            long.push_str("/segment");
        }
        long.push_str(&format!("/{}", "f".repeat(64)));
        long = long.trim_start_matches('/').to_owned();
        assert!(
            long.len() > 100,
            "这条路径应当超过 100 字节（实测 {}）",
            long.len()
        );
        let dir = std::env::temp_dir().join(format!("yanshi_tar_long_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("临时目录");
        let tar = write_tar(&[TarEntry {
            path: long.clone(),
            bytes: vec![1, 2, 3],
        }]);
        let path = dir.join("long.yanshi");
        std::fs::write(&path, &tar).expect("写包");
        let extracted = std::process::Command::new("tar")
            .args(["-xOf", path.to_str().unwrap(), &long])
            .output()
            .expect("tar 应当能解出");
        assert!(
            extracted.status.success(),
            "长路径应当能解出：{:?}",
            String::from_utf8_lossy(&extracted.stderr)
        );
        assert_eq!(extracted.stdout, vec![1, 2, 3]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
