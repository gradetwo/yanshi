//! 示范笔刷介质插件 —— 展示第三方如何按 **原始 wasm ABI** 写一个介质 ✓。
//!
//! 设计 11.1 规定：其他介质（油画、水彩、马克笔、铅笔、像素、矢量）通过 **WASM 插件**扩展 ✓，
//! 并给了插件边界：**无网络、注入确定性 PRNG、配额、D2 浮点等级** ✓。
//! 本 crate 就是这条边界的最小示范 ✓：
//! * **零依赖** ✓、**不导入任何宿主函数** ✓（`imports` 为空 ✓）⇒ 天然**无网络、无时钟** ✓。
//!   （这里用 `std` 只为拿到 `sqrt`/`clamp` 这类数学函数 ✓ —— wasm32-unknown-unknown 的 std
//!   不提供任何真正的宿主能力 ✓，而"能不能触网"由**宿主是否提供相应 import**决定 ✓，
//!   本插件一个 import 都没有 ✓，宿主会在加载时**校验 imports 为空** ✓。）
//! * 随机数**只**来自宿主传入的 seed ✓（内部 splitmix64 ✓）⇒ **确定性** ✓、同 seed 逐字节一致 ✓；
//! * 输出写在插件**自己的线性内存**的固定缓冲里 ✓，宿主通过 `yanshi_dab_ptr`/`yanshi_max_dab`
//!   读取并据此施加**配额** ✓；
//! * 浮点结果按设计属于 **D2 等级** ✓（不要求跨端逐位一致 ✓）。
//!
//! 编译（宿主侧无需任何工具链改动 ✓）：
//! ```sh
//! cargo build -p yanshi-medium-example --target wasm32-unknown-unknown --release
//! cp target/wasm32-unknown-unknown/release/yanshi_medium_example.wasm assets/mediums/example-dab.wasm
//! ```

/// ABI 版本：宿主必须核对 ✓（不匹配就拒绝加载 ✓）。
pub const ABI_VERSION: u32 = 1;

/// 单个"点"最大边长（像素）—— 宿主据此施加**配额** ✓。
pub const MAX_DAB: u32 = 64;

/// 固定输出缓冲：`MAX_DAB * MAX_DAB` 个 RGBA 字节 ✓。
static mut DAB: [u8; (MAX_DAB * MAX_DAB * 4) as usize] = [0; (MAX_DAB * MAX_DAB * 4) as usize];

/// 插件 ABI 版本 ✓。
#[no_mangle]
pub extern "C" fn yanshi_abi_version() -> u32 {
    ABI_VERSION
}

/// 输出缓冲地址（宿主用 `memory.buffer` 读取 ✓）。
#[no_mangle]
pub extern "C" fn yanshi_dab_ptr() -> u32 {
    core::ptr::addr_of!(DAB) as u32
}

/// 单个点允许的最大边长 ✓（宿主配额依据 ✓）。
#[no_mangle]
pub extern "C" fn yanshi_max_dab() -> u32 {
    MAX_DAB
}

/// splitmix64：完全确定、无状态依赖 ✓。
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// 生成一个"点"：`seed` 驱动、`radius` 控制软边 ✓。
///
/// 返回写入的字节数（`size * size * 4` ✓）；宿主据此读取缓冲 ✓。
/// **不使用任何导入函数** ⇒ 无法触网、无法读时钟 ✓。
#[no_mangle]
pub extern "C" fn yanshi_dab(seed: u32, size: u32, hardness_milli: u32) -> u32 {
    let size = if size == 0 {
        1
    } else if size > MAX_DAB {
        MAX_DAB
    } else {
        size
    };
    let hardness = (hardness_milli.min(1000)) as f32 / 1000.0;
    let bytes = size * size * 4;
    let mut state = (seed as u64) ^ 0x5DEE_CE66_D1CE_4E5B;
    // 用种子先摇一次，作为"每个点的颜色偏移"，让不同 seed 产出明显不同 ✓。
    let tint = splitmix64(&mut state) as u32;
    let radius = size as f32 / 2.0;
    let hard = radius * hardness;
    // SAFETY：单线程 wasm，且只写自己声明的固定缓冲 ✓。
    unsafe {
        let dab = &mut *core::ptr::addr_of_mut!(DAB);
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 + 0.5 - radius;
                let dy = y as f32 + 0.5 - radius;
                let distance = (dx * dx + dy * dy).sqrt();
                let coverage = if distance <= hard {
                    1.0
                } else if radius > hard {
                    let t = 1.0 - (distance - hard) / (radius - hard);
                    (t * t * (3.0 - 2.0 * t)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let index = ((y * size + x) * 4) as usize;
                // 颜色由 seed 决定（确定性 ✓），alpha 由覆盖率决定 ✓。
                let noise = splitmix64(&mut state) as u32;
                let r = (((tint >> 24) & 0xFF) as f32 * 0.5 + (noise >> 24) as f32 * 0.5) as u8;
                let g = (((tint >> 16) & 0xFF) as f32 * 0.5 + ((noise >> 16) & 0xFF) as f32 * 0.5)
                    as u8;
                let b =
                    (((tint >> 8) & 0xFF) as f32 * 0.5 + ((noise >> 8) & 0xFF) as f32 * 0.5) as u8;
                dab[index] = r;
                dab[index + 1] = g;
                dab[index + 2] = b;
                dab[index + 3] = (coverage * 255.0) as u8;
            }
        }
    }
    bytes
}
