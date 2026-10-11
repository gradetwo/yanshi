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

// **★★★ 逐 mode 的算术（**严格照抄 CPU 的 `blend_channel` ✓）★★★**（**第 628 轮 ✓）
//   **∴ 顺序与分支**必须逐一对齐**✗ ⇒ **∴ 任何改写都可能改变舍入** ✓
fn blend_channel(mode: u32, base: f32, source: f32) -> f32 {
    let cb = clamp(base, 0.0, 1.0);
    let cs = clamp(source, 0.0, 1.0);
    var out = 0.0;
    if (mode == 1u) { out = cb * cs; }                                  // Multiply
    else if (mode == 2u) { out = cb + cs - cb * cs; }                   // Screen
    else if (mode == 3u) {                                              // ColorDodge
        if (cs >= 1.0) { out = 1.0; } else { out = clamp(cb / (1.0 - cs), 0.0, 1.0); }
    }
    else if (mode == 4u) {                                              // Overlay
        if (cb <= 0.5) { out = 2.0 * cb * cs; }
        else { out = 1.0 - 2.0 * (1.0 - cb) * (1.0 - cs); }
    }
    else if (mode == 5u) { out = min(cb, cs); }                         // Darken
    else if (mode == 6u) { out = max(cb, cs); }                         // Lighten
    else if (mode == 7u) { out = min(cb + cs, 1.0); }                   // Add
    else if (mode == 8u) { out = max(cb - cs, 0.0); }                   // Subtract
    else { out = abs(cb - cs); }                                        // Difference
    return clamp(out, 0.0, 1.0);
}

// **∴ 预乘 → 直通 ✗**（**∴ 与 CPU 的 `unpremultiply` 逐行对应 ✓）★
//   **∴ 注意 ✗**：**分母是**传入的 alpha**（**即 `scaled[3]` 或 `dst[3]`，**未 clamp** ✓）★
fn unpremul(p: vec4<f32>) -> vec4<f32> {
    let alpha = p.w;
    if (alpha <= 0.0) { return vec4<f32>(0.0, 0.0, 0.0, 0.0); }
    return vec4<f32>(p.x / alpha, p.y / alpha, p.z / alpha, alpha);
}

// **∴ 单像素合成 ✗**（**∴ 与 CPU 的 `blend_pixel` 逐句对应 ✓）★
fn blend_one(mode: u32, src: vec4<f32>, dst: vec4<f32>) -> vec4<f32> {
    // **∴ Normal 用**未 clamp 的 alpha**✗（**∴ CPU 的 `over` 就是这样 ✓）★
    if (mode == 0u) {
        let alpha_s = src.w;
        if (alpha_s <= 0.0) { return dst; }
        if (alpha_s >= 1.0) { return src; }
        let inverse = 1.0 - alpha_s;
        return vec4<f32>(
            src.x + dst.x * inverse,
            src.y + dst.y * inverse,
            src.z + dst.z * inverse,
            src.w + dst.w * inverse,
        );
    }
    // **∴ 非 Normal ✗**：**alpha **clamp 过**** ✓
    let alpha_s = clamp(src.w, 0.0, 1.0);
    let alpha_b = clamp(dst.w, 0.0, 1.0);
    if (alpha_s <= 0.0) { return dst; }
    if (alpha_b <= 0.0) { return src; }
    let cs = unpremul(src);
    let cb = unpremul(dst);
    var out = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    for (var c = 0u; c < 3u; c = c + 1u) {
        let blended = blend_channel(mode, cb[c], cs[c]);
        out[c] = (1.0 - alpha_b) * alpha_s * cs[c]
            + (1.0 - alpha_s) * alpha_b * cb[c]
            + alpha_s * alpha_b * blended;
    }
    out.w = alpha_s + alpha_b * (1.0 - alpha_s);
    return out;
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= params.pixels) { return; }
    let b = i * 4u;
    // **★ opacity ✗ ★**：**CPU 的 `composite` **先做** `pixel * factor`**✗
    //   ＋ **∴ 而** `factor = opacity.clamp(0,1)`**✗ ⇒ **∴ Rust 侧已 clamp** ✓
    //     ＋ **∴ 顺序 ✗**：**4 个通道**各乘一次**（**与 CPU 一致 ✓）★
    let factor = params.extra.x;
    let scaled = vec4<f32>(
        src[b + 0u] * factor,
        src[b + 1u] * factor,
        src[b + 2u] * factor,
        src[b + 3u] * factor,
    );
    let dst_px = vec4<f32>(
        dst[b + 0u], dst[b + 1u], dst[b + 2u], dst[b + 3u],
    );
    // **∴ CPU 的 `composite` 有一个**跳过** ✗**：
    //   **∴ `scaled[3] <= 0.0 && mode == Normal` ⇒ **不写**（**保持 dst ✓）★
    //     ＋ **∴ 而** `blend_one` 在 Normal ＋ α≤0 时**也返回 dst** ✓
    //       ⇒ **∴ 所以**：**输出相同** ✓ ★
    let out_px = blend_one(params.mode, scaled, dst_px);
    out[b + 0u] = out_px.x;
    out[b + 1u] = out_px.y;
    out[b + 2u] = out_px.z;
    out[b + 3u] = out_px.w;
}

