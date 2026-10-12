// **★★★ 可分离高斯卷积（**滤镜 GPU 化 ✓）✗ ★★★**（**第 698 轮 ✓）
//
// **∴ 为什么它能**有机会逐位一致**✗ ★**（**第 697 轮查明 ✓）：
//   **∴ CPU 的 `convolve_horizontal` ✗**：
//     **∴ ① 行快照（**先读后写 ⇒ **∴ 无自反馈** ✓）
//       ＋ **∴ ② `source_x.clamp(0, width-1)`**✗（**∴ 边界复制 ✓）
//         ＋ **∴ ③ `acc[ch] += pixel[ch] * weight`**✗（**∴ 累加顺序 ＝ kernel 顺序 ✓）
//           ⇒ **∴ 所以**：**GPU 只要**用同样的顺序 ＋ 同样的边界**✗
//             ⇒ **∴ 理论上**f32 逐位相同** ✓ ★
//
// **∴ 两个 entry point ✗**：
//   **∴ `horizontal` ✗**：**沿 x 卷积 ✓
//     ＋ **∴ `vertical` ✗**：**沿 y 卷积 ✓
//       ⇒ **∴ 两个 pass ⇒ **可分离 ⇒ O(2r) 而不是 O(r²)** ✓ ★
//
// **∴ 核权重与归一化**在 CPU 侧算好**✗**（**∴ 与 CPU **同一公式 ＋ 同一顺序 ✓）
//   ⇒ **∴ GPU 只做**加权和** ✓ ★

struct Params {
    // 图像尺寸（像素）
    width: u32,
    height: u32,
    // 核半径（kernel 长度 ＝ 2*radius+1）
    radius: u32,
    // 核长度（显式传，避免 shader 里再算）
    kernel_len: u32,
};

@group(0) @binding(0) var<storage, read> src: array<f32>;
@group(0) @binding(1) var<storage, read_write> dst: array<f32>;
@group(0) @binding(2) var<storage, read> kernel: array<f32>;
@group(0) @binding(3) var<uniform> params: Params;

/// **∴ 水平卷积 ✗**（**∴ 与 CPU `convolve_horizontal` 逐字段对应 ✓）★
@compute @workgroup_size(64)
fn horizontal(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pixels = params.width * params.height;
    let index = gid.x;
    if (index >= pixels) {
        return;
    }
    let x = index % params.width;
    let y = index / params.width;
    // **∴ 累加顺序 ＝ kernel 顺序 ✗**（**∴ 与 CPU 相同 ✓）★
    var acc = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    for (var i: u32 = 0u; i < params.kernel_len; i = i + 1u) {
        // **∴ 边界复制 ✗**（**∴ 与 CPU 的 `clamp(0, width-1)` 相同 ✓）★
        let sx = clamp(i32(x) + i32(i) - i32(params.radius), 0, i32(params.width) - 1);
        let base = (y * params.width + u32(sx)) * 4u;
        let w = kernel[i];
        acc = acc + vec4<f32>(src[base], src[base + 1u], src[base + 2u], src[base + 3u]) * w;
    }
    let out_base = index * 4u;
    dst[out_base] = acc.x;
    dst[out_base + 1u] = acc.y;
    dst[out_base + 2u] = acc.z;
    dst[out_base + 3u] = acc.w;
}

/// **∴ 垂直卷积 ✗**（**∴ 与 CPU `convolve_vertical` 逐字段对应 ✓）★
@compute @workgroup_size(64)
fn vertical(@builtin(global_invocation_id) gid: vec3<u32>) {
    let pixels = params.width * params.height;
    let index = gid.x;
    if (index >= pixels) {
        return;
    }
    let x = index % params.width;
    let y = index / params.width;
    var acc = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    for (var i: u32 = 0u; i < params.kernel_len; i = i + 1u) {
        // **∴ 边界复制 ✗**（**∴ 与 CPU 的 `clamp(0, height-1)` 相同 ✓）★
        let sy = clamp(i32(y) + i32(i) - i32(params.radius), 0, i32(params.height) - 1);
        let base = (u32(sy) * params.width + x) * 4u;
        let w = kernel[i];
        acc = acc + vec4<f32>(src[base], src[base + 1u], src[base + 2u], src[base + 3u]) * w;
    }
    let out_base = index * 4u;
    dst[out_base] = acc.x;
    dst[out_base + 1u] = acc.y;
    dst[out_base + 2u] = acc.z;
    dst[out_base + 3u] = acc.w;
}
