//! **★ 量一量"轮询跟尾"的空转开销（2026-10-15 ✓）★**
//!
//! **为什么要有它** ✗：用户对 `--watch` 的触发机制有两个要求 ——
//! ① 更倾向**平台监听**（"轮询还是不优雅" ✓）；② 但**先担心浪费 CPU** ✓
//! ⇒ ⇒ 所以**先量开销，再决定** ✓（不许凭感觉说"轮询很便宜" ✗）。
//!
//! **怎么量** ✓：按**真实节奏**空转（`--interval-ms` ✓ 默认 200 ms ＝ 5 Hz ✓），
//! 期间**只有 `stat` ＋ 一次 `open`/`read`** ✓，跑 `--seconds` 秒 ✓，打印 **tick 数** ✓。
//! **外部证据** ✓ 由调用方给：
//!   * `/usr/bin/time -v` ⇒ **user/sys CPU 时间** ✓（这才是"浪费 CPU"的直接读数 ✓）；
//!   * `strace -c -f` ⇒ **系统调用次数** ✓（每 tick 应当是常数个小调用 ✓）。
//!
//! **两种节奏都要量** ✓：① **单文档**（1 个 follower ✓）；② **多文档**（N 个 ✓ ——
//! 这才是真正的风险面 ✗：N 个文档 ⇒ N 个 follower ⇒ N × 5 Hz 次 stat ✓）。
//!
//! 用法 ✓：
//! ```text
//! cargo run --release -p yanshi-core --example watch_overhead -- --docs 1
//! cargo run --release -p yanshi-core --example watch_overhead -- --docs 50
//! ```
use std::io::Write;
use std::time::{Duration, Instant};

use yanshi_core::watch::TailFollower;

fn arg(name: &str, default: u64) -> u64 {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|x| x == name)
        .and_then(|i| a.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let docs = arg("--docs", 1).max(1) as usize;
    let interval_ms = arg("--interval-ms", 200).max(1);
    let seconds = arg("--seconds", 5).max(1);
    let dir = std::env::temp_dir().join(format!("yanshi-watch-overhead-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");

    // 每个"文档"一个 atoms.jsonl ✓（内容大小不影响 stat 成本 ✓，但**有文件**更接近真实 ✓）。
    let mut followers = Vec::with_capacity(docs);
    for i in 0..docs {
        let path = dir.join(format!("atoms-{i}.jsonl"));
        let mut f = std::fs::File::create(&path).expect("建文件");
        writeln!(
            f,
            r#"{{"id":"a{i}","seq":1,"kind":"create_document","actor":"human:web","session":"s","timestamp":1,"schema_version":1,"payload":{{"background":{{"a":255,"b":255,"g":255,"r":255}},"color_space":"srgb","doc_id":"d{i}","height":10,"width":10}}}}"#
        )
        .expect("写一行");
        followers.push(TailFollower::new(&path));
    }

    println!(
        "  跟 {docs} 个文档｜间隔 {interval_ms} ms（{:.1} Hz）｜跑 {seconds} 秒 ⇒ 共 {} 次 poll",
        1000.0 / interval_ms as f64,
        docs as u64 * seconds * 1000 / interval_ms
    );
    let t0 = Instant::now();
    let mut ticks = 0u64;
    let mut applied = 0u64;
    while t0.elapsed() < Duration::from_secs(seconds) {
        for f in followers.iter_mut() {
            if let Ok(p) = f.poll() {
                applied += p.atoms.len() as u64;
            }
        }
        ticks += 1;
        std::thread::sleep(Duration::from_millis(interval_ms));
        // **刻意不打印** ✓：打印本身的开销会污染"空转"的读数 ✗。
    }
    let wall = t0.elapsed();
    println!(
        "  完成：{ticks} 轮 × {docs} 文档 ＝ {} 次 poll｜收到新原子 {applied} 条｜墙钟 {:.2} s",
        ticks * docs as u64,
        wall.as_secs_f64()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
