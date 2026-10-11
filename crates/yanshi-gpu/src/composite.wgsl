// **★★★ GPU 合成内核（**切片 1：Normal ✓）✗ ★★★**（**第 625 轮 ✓；目标第 11 条 ✓）
//
// **∴ 目标 ✗**：**把 `Buffer::composite` 的逐像素混合搬上 GPU** ✓
//   ＋ **∴ 从而**：**图层／合成不再往返 CPU** ✓ ★
//
// **★★★ 逐位一致是硬要求 ✗ ★★★**（**用户口径 ✓）：
//   **∴ 所以 ✗**：**内核的表达式顺序**必须与 CPU 的
//     `yanshi_render::blend` **逐一对应** ✓
//       ＋ **∴ 而 CPU 真值 ✗**（`Buffer::blend` ／ `blend_at` ✓）：
//         `out[c] = src[c] + dst[c] * (1.0 - src[3])`** ✓
//           ＋ **∴ 注意顺序 ✗**：**乘法在里、加法在外** ✓
//             ＋ **∴ 而**内核算 `1.0 - src[3]` 存成变量 ⇒ **∴ 与 CPU 一致** ✓ ★
//
// **★ 为什么只做 Normal（**切片 ✓）★**：
//   **∴ `Normal` 是**热路径**** ✗（**∴ 源码注释 ✓）
//     ＋ **∴ 且**它**无需反预乘**✗ ⇒ **∴ 数学最简（**4 乘加 ✓）
//       ⇒ **∴ 于是**：**它能**最快回答最大的未知**✗ —— **∴ FMA 会不会破坏逐位一致** ✓ ★
//
// **∴ 其余 9 种 mode ✗**：**留后续轮**（**∴ 它们**都要**反预乘 ＋ `blend_channel` ✓）
//
// **∴ 布局 ✗**：**与 `quantize.wgsl` 同风格（**4 个 binding ＋ 32 B uniform ✓）★

struct Params {
    pixels: u32,   // **∴ 像素数 ✓
    mode: u32,     // **∴ 0 ＝ Normal（**后续轮扩 ✓）
    _pad0: u32,
    _pad1: u32,
    // **∴ 预留 ✗**：**后续轮放** opacity 等** ✓
    extra: vec4<f32>,
};

@group(0) @binding(0) var<storage, read> src: array<f32>;   // **∴ 源（**预乘 f32 RGBA ✓）
@group(0) @binding(1) var<storage, read> dst: array<f32>;   // **∴ 背景（**预乘 ✓）
@group(0) @binding(2) var<storage, read_write> out: array<f32>;  // **∴ 结果 ✓
@group(0) @binding(3) var<uniform> params: Params;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= params.pixels) { return; }
    let b = i * 4u;
    // **★ 与 CPU 的 `blend_at` **逐运算对应** ✗ ★**：
    //   **∴ CPU ✗**：`let inverse = 1.0 - source[3];`
    //     ＋ **∴ 然后** `source[c] + self.pixels[c] * inverse` ✓
    //       ⇒ **∴ 内核 ✗**：**先算 `inverse`**✗ ＋ **再** `src + dst * inverse`** ✓
    //         ＋ **∴ 严禁 ✗**：**写成一行的 `fma(dst, inverse, src)`**✗
    //           ⇒ **∴ 因为**那**会**改变舍入** ⇒ **∴ 判据会红** ✓ ★
    // **★★★ 必须复刻 CPU 的**早退分支** ✗ ★★★**（**第 625 轮 ✓）
    //   **∴ CPU 真值 ✗**：`over()` 有三个分支**✗
    //     **∴ ① `alpha_s <= 0.0` ⇒ 返回 backdrop**✗
    //       ＋ **∴ 若**内核不算它 ⇒ **∴ 会**返回 `src + dst`**✗
    //         ⇒ **★ 那是**真实差异**（**α ＝ 0 但 RGB 非 0 时 ✓）★
    //     **∴ ② `alpha_s >= 1.0` ⇒ 返回 source**✗
    //       ＋ **∴ 而**内核算 `src + dst * 0.0`**✗ ⇒ **∴ 结果**等于 `src`** ✓（**∴ 浮点上一致 ✓）
    //         ⇒ **∴ 但**仍**显式写出**（**∴ 与 CPU **逐句对应** ⇒ 更安全 ✓）★
    //     **∴ ③ 否则 ✗**：`inverse ＝ 1.0 - alpha_s` ＋ `src[c] + dst[c] * inverse` ✓ ★
    //   **∴ 教训 ✗**：**「逐位一致」不只是**表达式顺序**✗
    //     ＋ **∴ 还包括**所有早退分支**（**它们**改变结果 ✓）★
    if (src[b + 3u] <= 0.0) {
        out[b + 0u] = dst[b + 0u];
        out[b + 1u] = dst[b + 1u];
        out[b + 2u] = dst[b + 2u];
        out[b + 3u] = dst[b + 3u];
        return;
    }
    if (src[b + 3u] >= 1.0) {
        out[b + 0u] = src[b + 0u];
        out[b + 1u] = src[b + 1u];
        out[b + 2u] = src[b + 2u];
        out[b + 3u] = src[b + 3u];
        return;
    }
    let inverse = 1.0 - src[b + 3u];
    let r = src[b + 0u] + dst[b + 0u] * inverse;
    let g = src[b + 1u] + dst[b + 1u] * inverse;
    let bl = src[b + 2u] + dst[b + 2u] * inverse;
    let a = src[b + 3u] + dst[b + 3u] * inverse;
    out[b + 0u] = r;
    out[b + 1u] = g;
    out[b + 2u] = bl;
    out[b + 3u] = a;
}
