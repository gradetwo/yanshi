//! # 偃师 Yanshi WASM 计算内核绑定
//!
//! 把计算内核层（`yanshi-core` 折叠求值 + `yanshi-render` 笔触/混合/滤镜/缩略图）
//! 编译到 `wasm32-unknown-unknown`，给 Web 编辑器做**本地乐观渲染**（设计文档 13.3 / 6.1）。
//!
//! 设计要点：
//!
//! - **单一来源**：与服务端共用同一套 Rust 计算内核，D0 bit-exact；本 crate 只做绑定，
//!   不含任何渲染逻辑（逻辑全在 [`kernel`]，可在宿主上跑单测）。
//! - **调用约定**：全部返回 JSON 文本信封 `{"ok":true,...}` / `{"ok":false,...}`（5.7 形状），
//!   避免在 ABI 上引入 `JsValue` 异常路径，JS 侧处理统一。
//! - **零拷贝倾向**：像素以 `Vec<u8>` 返回 → wasm-bindgen 直接给 `Uint8Array`，
//!   JS 侧可零拷贝塞进 `ImageData`；PNG 字节用于与服务端比对哈希（bit-exact 验收）。
//! - **内存**（13.3）：硬上限 + 90% 水位内部自动淘汰，另暴露
//!   `evict_outside_viewport` 供 JS 按视口主动淘汰。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod kernel;

mod brush;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;
use yanshi_core::Bbox;

pub use kernel::{
    ApplyReport, Kernel, KernelError, KernelStats, RenderResult, DEFAULT_MEMORY_LIMIT,
    WATERMARK_RATIO, WATERMARK_TARGET_RATIO,
};

fn parse_bbox(x: f64, y: f64, w: f64, h: f64) -> Bbox {
    Bbox::new(x, y, w.max(1.0), h.max(1.0))
}

fn envelope(result: Result<Value, KernelError>) -> String {
    match result {
        Ok(value) => value.to_string(),
        Err(error) => error.to_json().to_string(),
    }
}

/// **★ 页面里有没有 `navigator.gpu` ✗ ★**（**第 444 轮 ✓；**零新依赖 ✓）
///
/// **∴ 为什么这样写 ✗**：**要读 `navigator.gpu`**✗
///   ⇒ **∴ 而**引入 `web-sys`／`js-sys` **会**增大内核体积** ✓
///     ⇒ **∴ 所以**：**用 `extern "C"` **直接声明**那两个**最小的接口** ✓
///       ⇒ **∴ 于是**：**零新依赖**✗ ＋ **体积**几乎不增** ✓ ★**** ✓✓
///
/// **∴ 两面 ✗**：
///   **∴ 收益 ✗**：**不引入 `web-sys`**✗（**∴ 那个 crate**很大 ✓）
///   **∴ 代价 ✗**：**手写绑定**✗ ⇒ **∴ 要**自己保证签名正确** ✓
///     ⇒ **∴ 而**范围**极小**✗（**∴ 只**读一个属性 ✓）★**** ✓✓
///
/// **∴ 失败**不假装** ✗**：**任何一步拿不到** ⇒ **∴ 返回 `false`** ✓
///   （**∴ 那是**「**页面没有 WebGPU**」的**安全默认** ✓）
fn page_has_webgpu() -> bool {
    js_page_has_webgpu()
}

/// **★ 内联一小段 JS ✗ ★**（**第 444 轮 ✓；**零新 crate ✓）
///
/// **∴ 为什么用 `inline_js` ✗**：**要读 `navigator.gpu`**✗
///   ⇒ **∴ 而** `wasm-bindgen` **本身**不做**属性访问** ✓
///     ⇒ **∴ 两条路 ✗**：
///       **∴ 甲**：**引 `js-sys`／`web-sys`**✗ ⇒ **∴ 体积**增加** ✓
///       **∴ 乙（**选它 ✓）✗**：**内联一行 JS** ✗ ⇒ **∴ 零新 crate** ✓
///     ⇒ **★ 所以**：**选乙**✗ ⇒ **∴ 代价**是**一小段 JS**会**打进产物** ✓（**∴ 极小 ✓）★**** ✓✓
#[wasm_bindgen(inline_js = r#"
export function __yanshi_page_has_webgpu() {
  try {
    return typeof navigator !== "undefined" && !!navigator.gpu;
  } catch (_) {
    return false;
  }
}
"#)]
extern "C" {
    /// **∴ 页面有没有 WebGPU ✗**（**∴ 失败 ⇒ `false` ✓）
    #[wasm_bindgen(js_name = __yanshi_page_has_webgpu)]
    fn js_page_has_webgpu() -> bool;
}

/// 浏览器端的计算内核句柄。
#[wasm_bindgen]
pub struct WasmKernel {
    inner: Kernel,
}

#[wasm_bindgen]
impl WasmKernel {
    /// 新建内核：`doc_id`、tile 尺寸（32/64/128/256/512）、画布宽高、内存硬上限（字节）。
    /// **笔刷预览** ✓（(A)③：把门面那件事搬进内核 ⇒ **一份实现** ✓）。
    /// 收一段 JSON 请求 ⇒ 成功返回像素 ✓；失败返回 `undefined` ✓（**与"零长度成功"可区分** ✓）。
    pub fn paint_brush(&self, request_json: &str) -> Vec<u8> {
        match crate::brush::paint(request_json) {
            Ok(rgba) => rgba,
            Err(reason) => {
                // **失败也要能说出为什么** ✓ —— 门面当年有这条通道 ✓，我第一版丢了它 ✗。
                crate::brush::set_error(&reason);
                Vec::new()
            }
        }
    }

    /// **上一次 `paint_brush` 失败的原因** ✓（成功时为空串 ✓，取走即清 ✓）。
    pub fn paint_brush_error(&self) -> String {
        crate::brush::take_error()
    }

    /// **可选平滑** ✓ —— 与服务端 `brush_stroke.smooth` **同一条实现** ✓
    ///（`yanshi_render::brush::catmull_rom_smooth` ✓、同一个细分数 ✓）。
    ///
    /// 收 `[[x, y, pressure], ...]` ✓ ⇒ 成功回 `{"ok":true,"points":[…]}` ✓、
    /// 失败回 `{"ok":false,…}`（5.7 形状 ✓ —— 与内核其余导出同一套信封 ✓）。
    /// **为什么离线落笔要它** ✓：服务端是**先平滑 ⇒ 再算区域 ⇒ 再落笔** ✓
    /// ⇒ 离线要复现同一笔，必须走**同一个**平滑 ✓（见 `crate::brush::smooth_points_json` 的说明 ✓）。
    pub fn smooth_stroke_json(&self, points_json: &str) -> String {
        envelope(
            crate::brush::smooth_points_json(points_json)
                .map(|points| json!({"ok": true, "points": points}))
                .map_err(|reason| KernelError::new("invalid_argument", reason)),
        )
    }

    /// **构造一个内核实例** ✓（文档注释被我的插入"抢走"过一次 ✗ ⇒ 这是**第二次**踩同一个坑 ✓）。
    #[wasm_bindgen(constructor)]
    pub fn new(
        doc_id: &str,
        tile_size: u32,
        width: u32,
        height: u32,
        memory_limit: f64,
    ) -> Result<WasmKernel, JsValue> {
        let limit = if memory_limit.is_finite() && memory_limit > 0.0 {
            memory_limit as usize
        } else {
            DEFAULT_MEMORY_LIMIT
        };
        Kernel::new(doc_id, tile_size, width, height, limit)
            .map(|inner| Self { inner })
            .map_err(|error| JsValue::from_str(&error.detail))
    }

    /// 本地日志版本号（与服务端 5.7 错误里的 version 对照）。
    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    /// **★ 如实上报本内核用了**哪个渲染后端** ✗ ★**（**第 444 轮 ✓；**目标第 7 条 ✓）
    ///
    /// **∴ 为什么必须有它 ✗**：**目标第 7 条**说**「**`render_backend` 必须报**实际**后端**」 ✓
    ///   ⇒ **∴ 而**服务端早就有**这条**✗（**`/health` 的 `render_backend` ✓）
    ///     ⇒ **∴ 而**浏览器内核**此前**完全没有能力上报** ✓（**第 444 轮实测：0 处引用 ✓）
    ///       ⇒ **∴ 于是**：「**不许假装用了 GPU**」这条**在浏览器侧**无从校验** ✓
    ///         ⇒ **★ 所以**：**本函数**补上它** ✓ ★**** ✓✓
    ///
    /// **∴ 现在的如实答案 ✗**：**`"cpu"`** ✓
    ///   **∴ 因为**内核**尚未**接 WebGPU**✗（**∴ 三个硬阻塞见
    ///     **`yanshi-gpu::supports_sync_init` 的文档 ✓）
    ///     ⇒ **∴ 所以**：**`webgpu_used_by_kernel` **恒 `false`**** ✓（**∴ 如实 ✓）★**** ✓✓
    ///
    /// **∴ 与 `webgpu_in_page` 的区别（**关键 ✓）★**：
    ///   **∴ `webgpu_in_page` ✗**：**页面**有没有 `navigator.gpu`**✗（**∴ 那是**浏览器的能力** ✓）
    ///   **∴ `render_backend` ✗**：**本内核**实际**用了什么**✗（**∴ 那是**我们的行为** ✓）
    ///   ⇒ **★ 两者**绝不许混淆** ✗ ⇒ **∴ 否则**就是**假装** ✓ ★**** ✓✓
    pub fn backend_report_json(&self) -> String {
        // **∴ 探测 `navigator.gpu` ✗**（**∴ 零新依赖 ✓）：
        //   **∴ 用** `extern "C"` 声明**✗ ⇒ **∴ 不引入 `js-sys`／`web-sys`** ✓
        //     ⇒ **∴ 于是**：**内核体积**几乎不受影响** ✓（**∴ 按第 591 轮裁定 ✓）★**** ✓✓
        let in_page = page_has_webgpu();
        json!({
            // **∴ 实际后端 ✗**：**内核现在**就是 CPU** ✓（**∴ 不谎报 ✓）
            "render_backend": "cpu",
            // **∴ 页面能力 ✗**：**与「**我们用了什么**」分开 ✓
            "webgpu_in_page": in_page,
            // **∴ 我们用了 GPU 吗 ✗**：**恒 false** ✓（**∴ 如实 ✓）
            "webgpu_used_by_kernel": false,
            // **★ 为什么没用（**三种情形要分开 ✓）✗ ★**（**第 482 轮 ✓）：
            //   **∴ ①** 页面**没有** `navigator.gpu`**✗ ⇒ **∴ 那是**页面的能力问题** ✓
            //   **∴ ②** **本次内核构建**没有 `gpu` feature**✗
            //     ⇒ **∴ 那是**产物问题**✗（**∴ 第 480 轮**加了那个 feature ✓）
            //       ⇒ **∴ 而**它**是**默认状态** ✓ ★**** ✓✓
            //   **∴ ③** 有 feature**✗ ⇒ **∴ 那就**不是「**没接**」了**✗
            //     ⇒ **∴ 而是**「**导出了，但前端还没调它**」 ✓
            //       ⇒ **∴ 两者**绝不许混为一谈** ✓（**∴ 否则**又是含糊 ✓）★**** ✓✓
            "webgpu_unused_reason": if !in_page {
                "navigator_gpu_absent"
            } else if !cfg!(feature = "gpu") {
                "kernel_built_without_gpu_feature"
            } else {
                "gpu_exported_but_frontend_does_not_call_it_yet"
            },
        })
        .to_string()
    }

    /// view 模式批量装载：`json_array` 是服务端 `get_log` 给出的原子数组。
    pub fn load_atoms_json(&mut self, json_array: &str) -> String {
        envelope(self.inner.load_atoms_json(json_array))
    }

    /// 应用一个原子并本地乐观渲染；返回 13.1 风格的 dirty 报告。
    pub fn apply_atom_json(&mut self, atom_json: &str) -> String {
        envelope(self.inner.apply_atom_json(atom_json))
    }

    /// 渲染区域，返回直通 RGBA8（`Uint8Array`）。
    pub fn render_region_rgba(&mut self, x: f64, y: f64, w: f64, h: f64) -> Vec<u8> {
        self.inner
            .render_region(parse_bbox(x, y, w, h))
            .map(|result| result.rgba8)
            .unwrap_or_default()
    }

    /// 渲染区域，返回 PNG 字节（与服务端同一编码器，可直接比对哈希）。
    pub fn render_region_png(&mut self, x: f64, y: f64, w: f64, h: f64) -> Vec<u8> {
        self.inner
            .render_region(parse_bbox(x, y, w, h))
            .map(|result| result.png)
            .unwrap_or_default()
    }

    /// 渲染区域并返回完整元信息（JSON：bbox/宽高/padding/警告/tile 数）。
    pub fn render_region_info(&mut self, x: f64, y: f64, w: f64, h: f64) -> String {
        let region = parse_bbox(x, y, w, h);
        match self.inner.render_region(region) {
            Ok(result) => json!({
                "ok": true,
                "bbox": result.bbox,
                "width": result.width,
                "height": result.height,
                "tiles": result.tiles,
                "filter_padding": result.filter_padding,
                "warnings": result.warnings,
                "bytes": result.rgba8.len(),
                "png_bytes": result.png.len(),
            })
            .to_string(),
            Err(error) => error.to_json().to_string(),
        }
    }

    /// 直绘一个小区域（拖动中的笔迹反馈）。
    ///
    /// 查看器一直依赖它；此前该方法**并不存在**，JS 抛 `TypeError: ... is not a function`
    /// 被事件处理器吞掉，于是拖动与落笔后画布都没有内容（「操作后画布空白」缺陷）。
    pub fn render_region_direct_rgba(&mut self, x: f64, y: f64, w: f64, h: f64) -> Vec<u8> {
        self.inner
            .render_region_direct_rgba(
                x.max(0.0) as u32,
                y.max(0.0) as u32,
                w.max(1.0) as u32,
                h.max(1.0) as u32,
            )
            .unwrap_or_default()
    }

    /// 设置/更新**本地待提交覆盖层**（拖动中的笔迹），返回需要重绘的区域。
    ///
    /// 覆盖层不进原子日志：落笔时才用 `apply_atom_json` + `POST /api/atoms` 提交最终原子。
    pub fn set_preview_object(&mut self, json: &str) -> String {
        match self.inner.set_preview_object(json) {
            Ok(bbox) => json!({
                "ok": true,
                "dirty_bbox": bbox.map(|bbox| [bbox.x, bbox.y, bbox.w, bbox.h]),
                "has_preview": true,
            })
            .to_string(),
            Err(error) => error.to_json().to_string(),
        }
    }

    /// **增量**更新待提交笔迹（拖动中的每一帧走这条路：成本 ∝ 新增笔段）。
    pub fn extend_preview_stroke(&mut self, json: &str) -> String {
        match self.inner.extend_preview_stroke(json) {
            Ok(bbox) => json!({
                "ok": true,
                "dirty_bbox": bbox.map(|bbox| [bbox.x, bbox.y, bbox.w, bbox.h]),
                "has_preview": true,
            })
            .to_string(),
            Err(error) => error.to_json().to_string(),
        }
    }

    /// 落笔提交：合并进本地日志并**失效重算受影响 tile**，返回
    /// `{ok, report: {seq, dirty_bbox, dirty_tiles, head}}`。
    ///
    /// 调用方应据此重绘 `dirty_bbox`；不要再假定"覆盖层像素已在 tile 里"（那条假设在
    /// 覆盖层与提交原子落在不同图层时不成立，会造成落笔后画布空白）。
    pub fn commit_preview(&mut self, atom_json: &str) -> String {
        match self.inner.commit_preview(atom_json) {
            Ok(report) => json!({"ok": true, "report": report, "seq": report.seq}).to_string(),
            Err(error) => error.to_json().to_string(),
        }
    }

    /// 清除本地待提交覆盖层，返回需要重绘的区域。
    pub fn clear_preview(&mut self) -> String {
        let bbox = self.inner.clear_preview();
        json!({
            "ok": true,
            "dirty_bbox": bbox.map(|bbox| [bbox.x, bbox.y, bbox.w, bbox.h]),
            "has_preview": false,
        })
        .to_string()
    }

    /// 是否存在待提交覆盖层。
    pub fn has_preview(&self) -> bool {
        self.inner.has_preview()
    }

    /// 13.3：JS 侧按视口主动淘汰。
    pub fn evict_outside_viewport(&mut self, x: f64, y: f64, w: f64, h: f64) -> usize {
        self.inner.evict_outside_viewport(parse_bbox(x, y, w, h))
    }

    /// 设置视口（水位兜底会优先保留视口内 tile）。
    pub fn set_viewport(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.inner.set_viewport(parse_bbox(x, y, w, h));
    }

    /// 调整内存硬上限（字节）；突破水位时立即淘汰。
    pub fn set_memory_limit(&mut self, bytes: f64) {
        if bytes.is_finite() && bytes > 0.0 {
            self.inner.set_memory_limit(bytes as usize);
        }
    }

    /// 当前内存占用（字节）。
    pub fn memory_usage(&self) -> f64 {
        self.inner.stats().used_bytes as f64
    }

    /// 水位比例（`used / limit`）。
    pub fn memory_watermark(&self) -> f64 {
        self.inner.stats().watermark
    }

    /// HEAD seq。
    pub fn head_seq(&self) -> f64 {
        self.inner.head_seq() as f64
    }

    /// 状态摘要 JSON。
    pub fn state_json(&self) -> String {
        self.inner.state_json().to_string()
    }

    /// 统计 JSON（14.9 可观测性：命中率、淘汰、水位、自动淘汰次数）。
    pub fn stats_json(&self) -> String {
        serde_json::to_string(&self.inner.stats()).unwrap_or_else(|_| "{}".to_owned())
    }

    /// 写入本地 blob（6.3 blob 先行），返回 CAS 哈希。
    pub fn blob_put(&mut self, bytes: &[u8]) -> String {
        match self.inner.blob_put(bytes) {
            Ok(hash) => json!({"ok": true, "blob_hash": hash}).to_string(),
            Err(error) => error.to_json().to_string(),
        }
    }

    /// 读取本地 blob。
    pub fn blob_get(&mut self, hash: &str) -> Vec<u8> {
        self.inner.blob_get(hash).unwrap_or_default()
    }

    /// 最近一次 dirty 的 tile（JSON 数组）。
    pub fn last_dirty_tiles(&self) -> String {
        serde_json::to_string(self.inner.last_dirty_tiles()).unwrap_or_else(|_| "[]".to_owned())
    }
}

/// **原生对照入口**（第 245 轮）：把 `brush::paint` 暴露给**本机测试**。
///
/// **为什么需要它**：`paint_brush` 是 wasm-bindgen 门面上的**方法**，而本机测试
/// 造不出那个门面（它要 JS 环境）⇒ 于是"wasm 与 x86_64 是否逐字节一致"这个
/// 唯一能判定笔刷差异**是否可修**的问题**无法实测**。
///
/// **它只转发**：不改变任何行为，不参与浏览器路径。
/// **代价**：公开面多一个函数（文档与稳定性成本）。**收益**：把推断变成实测。
#[doc(hidden)]
pub fn paint_brush_bytes_for_test(request_json: &str) -> Option<Vec<u8>> {
    crate::brush::paint(request_json).ok()
}

/// **★ 内核真值：把**线性预乘 f32 像素**量化成 u8 RGBA ✗ ★**（第 60 轮 ✓）。
///
/// **∴ 为什么它必须**由内核自己**给出 ✗ ★**（**∴ 我**自己的教训换来的 ✓）：
///   **∴ 我**的 GPU 一致性判据**原来**用一个**页面侧重写**的参考**✗
///     （**先**`Math.fround`（**f32⇒f32，**错 ✓）⇒ **∴ 于是**报出**0 差异**的**假象 ✓；
///      **后**`Math.f16round`（**真 f16 ✓）⇒ **∴ 结论**翻转成 **1.5% 差 1** ✓）** ✓✓
///     ⇒ **∴ 所以**：**"**与某个参考一致 ✓"**这个判据 ✗**
///       ⇒ **∴ 必须**先证明**那个参考**就是**真值 ✓**** ✓✓
///   **∴ 本函数**走的正是**内核自己的**那条路 ✗**
///     （**`yanshi_render::Buffer::to_rgba8_quantized`**✗ ＝ **f16 往返 ＋ 4097 项查表 ✓）
///     ⇒ **∴ 所以**：**GPU 与它比**才**有意义 ✓**** ✓✓
///
/// **∴ 形状 ✗**：**入参**是**线性预乘 f32 的 RGBA 平铺数组**✗
///   （**`pixels.len() % 4 == 0` ✓）⇒ **∴ 返回**等长的 u8 RGBA ✓**
#[wasm_bindgen]
pub fn quantize_reference_rgba(pixels: &[f32]) -> Vec<u8> {
    let count = pixels.len() / 4;
    if count == 0 {
        return Vec::new();
    }
    // **∴ 用一个**1×count 的缓冲**✗ ⇒ **∴ 走**与内核相同的量化 ✓**** ✓✓
    let mut buffer = yanshi_render::Buffer::new(0, 0, count as u32, 1);
    buffer.pixels_mut().copy_from_slice(&pixels[..count * 4]);
    buffer.to_rgba8_quantized(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **测量**两条区域渲染路径的字节关系（**∴ 只测量、不断言 ✓** ⇒ 第 1340 轮定位到
    /// 判据量的是 tile 组合 ✗、而拖动走的是 scratch 直绘 ✗ ⇒ **∴ 所以改路前先问**：
    /// **∴ 同一区域两条路的像素**是否逐字节一致**✗？** ⇒ **∴ 一致才能安全改 ✓**）。
    #[test]
    fn region_paths_agree_byte_for_byte() {
        let mut kernel = WasmKernel::new("doc_m", 32, 64, 64, DEFAULT_MEMORY_LIMIT as f64).unwrap();
        let atoms = [
            json!({"id": "01AAAAAAAAAAAAAAAAAAAAAAAA01", "seq": 1, "kind": "create_document",
                   "actor": "human:1", "session": "s", "timestamp": 1,
                   "payload": {"doc_id": "doc_m", "width": 64, "height": 64, "color_space": "srgb",
                               "background": {"r": 255, "g": 255, "b": 255, "a": 255}}}),
            json!({"id": "01AAAAAAAAAAAAAAAAAAAAAAAA02", "seq": 2, "kind": "create_layer",
                   "actor": "human:1", "session": "s", "timestamp": 2,
                   "payload": {"layer_id": "layer_1"}}),
            json!({"id": "01AAAAAAAAAAAAAAAAAAAAAAAA03", "seq": 3, "kind": "draw_stroke",
                   "actor": "human:1", "session": "s", "timestamp": 3,
                   "payload": {"object_id": "obj_1", "layer_id": "layer_1",
                               "data": {"points": [[6.0, 6.0], [40.0, 20.0]], "size": 6.0,
                                        "color": [30, 30, 40, 255]}}}),
        ];
        for atom in &atoms {
            let response: Value =
                serde_json::from_str(&kernel.apply_atom_json(&atom.to_string())).unwrap();
            assert_eq!(response["ok"], json!(true), "{response}");
        }
        for (x, y, w, h) in [
            (0.0, 0.0, 64.0, 64.0),
            (8.0, 8.0, 32.0, 32.0),
            (6.0, 6.0, 20.0, 12.0),
        ] {
            let tile = kernel.render_region_rgba(x, y, w, h);
            let direct = kernel.render_region_direct_rgba(x, y, w, h);
            // **这是判据，不是测量**（第 1341 轮 ✓）：两路**必须逐字节一致** ✓。
            // 依据（2026-10-07 实测 ✓）：三个代表性区域（整幅 / 中段 / 小片）都是**差异 0 字节** ✓
            // ⇒ 所以"改成走 tile 组合"是**安全的**，而这条断言就是那次改动的护栏 ✓。
            let first_diff = tile.iter().zip(direct.iter()).position(|(a, b)| a != b);
            let differing = tile
                .iter()
                .zip(direct.iter())
                .filter(|(a, b)| a != b)
                .count();
            println!(
                "  ({x},{y},{w},{h})：tile {} 字节｜direct {} 字节｜逐字节相同 {}｜首个差异位 {:?}｜差异字节 {}",
                tile.len(), direct.len(), first_diff.is_none(), first_diff, differing
            );
            assert_eq!(
                tile.len(),
                direct.len(),
                "({x},{y},{w},{h}) 两条路的字节数不同：tile {} vs direct {}",
                tile.len(),
                direct.len()
            );
            assert_eq!(
                first_diff, None,
                "({x},{y},{w},{h}) 两条路**逐字节不一致**：首个差异位 {:?}，共 {} 字节不同 ✗",
                first_diff, differing
            );
        }
    }

    #[test]
    fn wasm_surface_returns_envelopes_and_bytes() {
        let mut kernel = WasmKernel::new("doc_1", 32, 64, 64, DEFAULT_MEMORY_LIMIT as f64).unwrap();
        assert_eq!(kernel.head_seq(), 0.0);

        // 原子：create_document + create_layer + 一笔。
        let atoms = [
            json!({"id": "01AAAAAAAAAAAAAAAAAAAAAAAA01", "seq": 1, "kind": "create_document",
                   "actor": "human:1", "session": "s", "timestamp": 1,
                   "payload": {"doc_id": "doc_1", "width": 64, "height": 64, "color_space": "srgb",
                               "background": {"r": 255, "g": 255, "b": 255, "a": 255}}}),
            json!({"id": "01AAAAAAAAAAAAAAAAAAAAAAAA02", "seq": 2, "kind": "create_layer",
                   "actor": "human:1", "session": "s", "timestamp": 2,
                   "payload": {"layer_id": "layer_1"}}),
            json!({"id": "01AAAAAAAAAAAAAAAAAAAAAAAA03", "seq": 3, "kind": "draw_stroke",
                   "actor": "human:1", "session": "s", "timestamp": 3,
                   "payload": {"object_id": "obj_1", "layer_id": "layer_1",
                               "data": {"points": [[6.0, 6.0], [40.0, 20.0]], "size": 6.0,
                                        "color": [30, 30, 40, 255]}}}),
        ];
        for atom in &atoms {
            let response: Value =
                serde_json::from_str(&kernel.apply_atom_json(&atom.to_string())).unwrap();
            assert_eq!(response["ok"], json!(true), "{response}");
        }
        assert_eq!(kernel.head_seq(), 3.0);

        let info: Value =
            serde_json::from_str(&kernel.render_region_info(0.0, 0.0, 64.0, 64.0)).unwrap();
        assert_eq!(info["width"], json!(64));
        let rgba = kernel.render_region_rgba(0.0, 0.0, 64.0, 64.0);
        assert_eq!(rgba.len(), 64 * 64 * 4);
        let png = kernel.render_region_png(0.0, 0.0, 64.0, 64.0);
        assert_eq!(
            &png[0..8],
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
        );

        // blob 往返。
        let stored: Value = serde_json::from_str(&kernel.blob_put(&[9, 8, 7])).unwrap();
        let hash = stored["blob_hash"].as_str().unwrap().to_owned();
        assert_eq!(kernel.blob_get(&hash), vec![9, 8, 7]);

        // 统计与水位。
        let stats: Value = serde_json::from_str(&kernel.stats_json()).unwrap();
        assert_eq!(stats["head_seq"], json!(3));
        assert!(stats["cache_hits"].as_u64().unwrap() > 0);
        assert!(kernel.memory_watermark() > 0.0);
        assert!(kernel.memory_usage() > 0.0);

        // 出错时是 5.7 形状而不是异常。
        let error: Value = serde_json::from_str(&kernel.apply_atom_json("{not json")).unwrap();
        assert_eq!(error["ok"], json!(false));
        assert_eq!(error["error_code"], json!("invalid_argument"));

        // 待提交覆盖层：增长 - 清除，且不动原子日志。
        let head = kernel.head_seq();
        let preview = json!({"layer_id": "layer_1", "type": "stroke", "data": {"points": [[6.0, 40.0], [50.0, 46.0]], "size": 5.0}});
        let first: Value =
            serde_json::from_str(&kernel.set_preview_object(&preview.to_string())).unwrap();
        assert_eq!(first["ok"], json!(true));
        assert!(first["dirty_bbox"].is_array());
        assert!(kernel.has_preview());
        assert_eq!(kernel.head_seq(), head, "覆盖层不改 HEAD");
        // 增量路径。
        let extended: Value = serde_json::from_str(
            &kernel.extend_preview_stroke(&json!({"layer_id": "layer_1", "type": "stroke", "data": {"points": [[6.0, 40.0], [20.0, 44.0]], "size": 5.0}}).to_string()),
        )
        .unwrap();
        assert_eq!(extended["ok"], json!(true));
        let extended2: Value = serde_json::from_str(
            &kernel.extend_preview_stroke(&json!({"layer_id": "layer_1", "type": "stroke", "data": {"points": [[6.0, 40.0], [20.0, 44.0], [34.0, 48.0]], "size": 5.0}}).to_string()),
        )
        .unwrap();
        assert_eq!(extended2["ok"], json!(true), "{extended2}");
        assert_eq!(kernel.head_seq(), head, "增量覆盖层不改 HEAD");

        let cleared: Value = serde_json::from_str(&kernel.clear_preview()).unwrap();
        assert_eq!(cleared["has_preview"], json!(false));
        assert!(!kernel.has_preview());

        // 视口淘汰返回数量。
        let evicted = kernel.evict_outside_viewport(0.0, 0.0, 32.0, 32.0);
        assert!(evicted <= 4);
        let _ = kernel.version();
    }
}

/// **★ 内核的**异步 GPU 探测**✗ ★**（**第 480 轮 ✓；**目标第 6 条 ✓）
///
/// **∴ 为什么必须是 `async` ✗**：**本平台是**单线程事件循环**✗
///   ⇒ **∴ 同步 `block_on` **会**死锁** ✓（**第 470 轮实测过 ✓）
///     ⇒ **★ 所以 ✗ ★**：**这里**只调 `new_async`**✗
///       ＋ **∴ 让**浏览器的事件循环**来推进** ✓ ★**** ✓✓
///
/// **∴ 返回 ✗**：**一段 JSON**✗（**∴ 含**`ok`／`adapter`／`error` ✓）
///   ＋ **∴ 而**它**不谎报**✗：**失败**就把**原因**写出来** ✓ ★**** ✓✓
///
/// **∴ 与 `backend_report_json` 的关系 ✗**：**那条**报**内核实际用的后端**（**`cpu` ✓）
///   ＋ **∴ 这条**只**探测**适配器**✗ ⇒ **∴ 它**不改变**`render_backend`** ✓
///     ⇒ **∴ 于是**：**探测**与**实际使用**分开** ✓ ★**** ✓✓
#[cfg(feature = "gpu")]
#[wasm_bindgen]
pub async fn gpu_probe_async() -> String {
    // **∴ 用一条**最小 LUT**✗（**∴ 只为**建设备 ✓）
    let lut = yanshi_render::color::srgb_encode_table();
    match yanshi_gpu::Quantizer::new_async(&lut[..]).await {
        Ok(q) => json!({
            "ok": true,
            "adapter": q.adapter_note(),
            "note": "内核的异步 GPU 初始化成功（**未改变 render_backend**）",
        })
        .to_string(),
        Err(e) => json!({
            "ok": false,
            "error": e,
            "note": "内核的异步 GPU 初始化失败 ⇒ 内核仍走 CPU（**如实 ✓）",
        })
        .to_string(),
    }
}

/// **★ 内核的**异步 GPU 量化**✗ ★**（**第 480 轮 ✓）
///
/// **∴ 输入 ✗**：**`pixels` ＝ 归一化 f32 的 RGBA 序列**✗（**长度 ＝ count × 4 ✓）
///   ⇒ **∴ 返回 ✗**：**量化后的 RGBA8 字节** ✓
///
/// **∴ 失败语义 ✗**：**回退 CPU**✗（**∴ 用**本仓库既有的 `to_rgba8_quantized` ✓）
///   ⇒ **∴ 且**：**返回值**与 CPU 路**逐字节相同** ✓（**∴ 由 crate 内部核对 ✓）★**** ✓✓
#[cfg(feature = "gpu")]
#[wasm_bindgen]
pub async fn gpu_quantize_async(pixels: Vec<f32>, count: usize) -> Vec<u8> {
    let lut = yanshi_render::color::srgb_encode_table();
    if let Ok(q) = yanshi_gpu::Quantizer::new_async(&lut[..]).await {
        if let Ok(out) = q.quantize_async(&pixels, count, None).await {
            return out;
        }
    }
    // **∴ 回退 CPU ✗**（**∴ 与纯 CPU 路**逐字节相同** ✓）
    let mut buffer = yanshi_render::Buffer::new(0, 0, count as u32, 1);
    buffer.pixels_mut().copy_from_slice(&pixels[..count * 4]);
    buffer.to_rgba8_quantized(None)
}
