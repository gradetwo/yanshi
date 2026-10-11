// **★★★ 微基准：池复用时的三种清零写法 ✗ ★★★**（**第 668 轮 ✓）
//
// **∴ 背景 ✗**：**`Buffer::reset` 用 `clear() ＋ resize(len, 0.0)`**✗
//   ＋ **∴ 注释自述「每个元素都被写入 0.0」** ⇒ **∴ 那**是**逐元素写** ✓
//     ＋ **∴ 而**探针量到**每层池命中 2 ms**✗（**∴ 4K 分带 2400×700 ✓）
//       ⇒ **∴ 所以**值得比三种写法** ✓ ★
//
// **∴ 注意 ✗**：**`black_box` 在停表之后** ＋ **触碰数据**（**第 661 轮修正 ✓）★
use std::time::Instant;

const LEN: usize = 2400 * 700 * 4; // **∴ 4K 分带的图层缓冲 ✓

#[test]
fn bench_reset_zeroing() {
    let rounds = 7usize;
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut c = Vec::new();
    // **∴ 预分配（**模拟池已持有容量 ✓）
    let mut keep: Vec<f32> = Vec::with_capacity(LEN);
    for _ in 0..rounds {
        // **∴ ① 现状：clear ＋ resize(len, 0.0) ✓
        let t = Instant::now();
        keep.clear();
        keep.resize(LEN, 0.0);
        let d1 = t.elapsed().as_secs_f64() * 1000.0;
        a.push(d1);
        // **∴ ② fill(0.0)（**先保证长度 ✓）★
        let t = Instant::now();
        keep.clear();
        keep.resize(LEN, 0.0); // 长度补足
        keep.fill(0.0);
        let d2 = t.elapsed().as_secs_f64() * 1000.0;
        b.push(d2);
        // **∴ ③ vec![0.0; len]（**calloc ✓）
        let t = Instant::now();
        let fresh = vec![0.0f32; LEN];
        let d3 = t.elapsed().as_secs_f64() * 1000.0;
        c.push(d3);
        std::hint::black_box(keep.iter().fold(0.0f32, |s, v| s + *v));
        std::hint::black_box(fresh.iter().fold(0.0f32, |s, v| s + *v));
    }
    let med = |v: &mut Vec<f64>| {
        v.sort_by(|x, y| x.partial_cmp(y).unwrap());
        v[v.len() / 2]
    };
    let (ma, mb, mc) = (med(&mut a), med(&mut b), med(&mut c));
    println!("  ★ ① clear ＋ resize(len, 0.0)（**现状 ✓）中位 {ma:.2} ms");
    println!("  ★ ② clear ＋ resize ＋ fill(0.0)       中位 {mb:.2} ms");
    println!("  ★ ③ vec![0.0; len]（**新分配 ✓）        中位 {mc:.2} ms");
    println!(
        "  ⇒ 最快 {:?}",
        if mb < ma && mb < mc {
            "②"
        } else if mc < ma {
            "③"
        } else {
            "①"
        }
    );
}
