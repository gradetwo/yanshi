//! # 偃师 Yanshi 渲染计算内核层
//!
//! 本 crate 是设计文档 v1.0-draft4 第 6.1 节定义的**计算内核层**参考实现：
//! 纯 CPU、无 GPU 依赖、确定性（D0 bit-exact 基线），客户端与服务端共享同一份代码。
//! 合成后端层（tile 上传、显示合成、GPU）不在本 crate 内——这里只输出像素。
//!
//! | 模块 | 设计文档 | 职责 |
//! |---|---|---|
//! | [`half`] | 6.1 | IEEE-754 binary16 存储（内存 tile 用 f16 线性） |
//! | [`color`] | 6.1 | 线性光空间、预乘 alpha、sRGB 传递函数、u8 打包 |
//! | [`blend`] | 6.1 | 混合模式（W3C 合成公式，预乘 alpha） |
//! | [`prng`] | 6.1 / 11.1 | 由 seed 驱动的确定性 PRNG（抖动、纹理） |
//! | [`geometry`] | 8.1 | 覆盖率光栅化（矩形/椭圆/多边形）与 stamp 展开 |
//! | [`tile`] | 6.4 / 13.3 | Tile、TileGrid、TileCache（LRU、字节预算、视口淘汰） |
//! | [`buffer`] | 6.4 / 8.3 | 区域像素缓冲（渲染计算的物化载体） |
//! | [`object`] | 4.2 | 对象 → 渲染图元解析与效果包围盒 |
//! | [`brush`] | 11.1 | 通用光栅笔刷 stamping（间距、硬度、压力、流量、抖动） |
//! | [`curve`] | 808 行 `appearance` | 笔触曲线：size / opacity / pressure（分段线性求值） |
//! | [`dynamics`] | 808 行 `appearance` | 笔触动力学（抖动/散布/旋转）与程序化纹理（确定性 seed） |
//! | [`paint`] | 11.1 | 湿笔：载墨量衰减、湿度耗墨、混色 |
//! | [`font`] | 4.2 / 1175 | 内置 ASCII 位图字体与文本栅格化 |
//! | [`font_atlas`] | 1175 / 1287 | 内嵌 OFL 位图图集（ASCII + GB2312 一级字库），零依赖零 IO |
//! | [`filter`] | 4.2 | 调整与滤镜内核（模糊、亮度对比、饱和度、色阶、反相） |
//! | [`dirty`] | 6.6 | 几何 / 结构双 dirty 传播 → tile 失效集 |
//! | [`render`] | 6.2 / 8.3 | 文档渲染：图层隔离、区域渲染、位图补丁 |
//! | [`selection`] | 4.4 | 选区覆盖度几何（矩形/椭圆/多边形 + 羽化 + 反选 + 组合模式）|
//! | [`thumb`] | 7 章 | 缩略图管线与分块增量更新 |
//! | [`png`] | 18 章 | PNG 输出（零依赖、确定性编码器） |
//!
//! ## 确定性（D0）
//!
//! - 像素运算全部为标量 `f32`，运算顺序固定；存储为 f16（[`half`]），往返可复现。
//! - 随机量（抖动、纹理）只来自原子 `seed` 驱动的 [`prng`]，不使用系统时钟或线程调度。
//! - 合成在线性光空间进行、alpha 预乘；输出转换到显示空间（sRGB）后才量化到 u8。
//!
//! ```
//! use serde_json::json;
//! use yanshi_core::blob::MemoryBlobStore;
//! use yanshi_core::{Atom, AtomKind, AtomLog, FoldEngine};
//! use yanshi_render::render::Renderer;
//! use yanshi_render::tile::TileGrid;
//!
//! let mut log = AtomLog::new();
//! log.append(Atom::new(
//!     AtomKind::CreateDocument,
//!     "human:1",
//!     "s",
//!     json!({"doc_id": "doc_1", "width": 64, "height": 64,
//!            "background": {"r": 255, "g": 255, "b": 255, "a": 255}}),
//! ))
//! .unwrap();
//! log.append(Atom::new(
//!     AtomKind::CreateLayer,
//!     "human:1",
//!     "s",
//!     json!({"layer_id": "layer_1"}),
//! ))
//! .unwrap();
//! log.append(Atom::new(
//!     AtomKind::DrawStroke,
//!     "human:1",
//!     "s",
//!     json!({
//!         "object_id": "obj_1",
//!         "layer_id": "layer_1",
//!         "data": {"points": [[8.0, 8.0], [48.0, 48.0]], "color": [0.0, 0.0, 0.0, 1.0], "size": 6.0},
//!     }),
//! ))
//! .unwrap();
//!
//! let state = FoldEngine::new().fold(&log).unwrap().state;
//! let store = MemoryBlobStore::new();
//! let mut renderer = Renderer::new(TileGrid::new(32, 64, 64).unwrap());
//! let out = renderer
//!     .render_region(&state, &store, yanshi_core::Bbox::new(0.0, 0.0, 16.0, 16.0))
//!     .unwrap();
//! assert_eq!(out.width, 16);
//! assert_eq!(out.rgba8.len(), 16 * 16 * 4);
//! assert!(out.pixel(8, 8).unwrap()[0] < 200, "笔迹处比白底深");
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod blend;
pub mod brush;
pub mod buffer;
pub mod color;
pub mod curve;
pub mod dirty;
pub mod dynamics;
pub mod filter;
pub mod font;
pub mod font_atlas;
pub mod geometry;
pub mod half;
pub mod object;
pub mod paint;
pub mod png;
pub mod polygon;
pub mod prng;
pub mod render;
pub mod selection;
pub mod thumb;
pub mod tile;

pub use blend::{blend_pixel, over, scale_alpha, BlendMode};
pub use brush::{draw_stamp, stamp_stroke, BrushSpec, StrokeGeometry, StrokePoint};
pub use buffer::Buffer;
pub use color::{
    byte_to_linear, composite_over_background, linear_premul_to_u8x4, linear_to_byte,
    linear_to_byte_exact, linear_to_srgb, linear_to_srgb_fast, premultiply, srgb_to_linear,
    u8x4_to_linear_premul, unpremultiply, LinearRgba,
};
pub use dirty::{invalidated_tiles, plan_dirty, plan_dirty_with_log, DirtyKind, DirtySet};
pub use filter::{
    apply_adjustment, apply_filter, box_blur, brightness_contrast, color_balance, dehaze,
    estimate_atmospheric_light, gaussian_blur, invert, levels, saturation, split_toning,
    AdjustmentKind, FilterKind, ADJUSTMENT_NAMES, FILTER_NAMES,
};
pub use geometry::{
    dashed_line, ellipse_coverage, point_in_polygon, polygon_coverage, rect_coverage, Coverage,
    DEFAULT_SUPERSAMPLE,
};
pub use half::{f16_bits_to_f32, f32_to_f16_bits, quantize_f16};
pub use object::{layer_bbox, object_bbox, parse_color, parse_object, Primitive, ShapeKind};
pub use png::{adler32, encode_png, write_png_file};
pub use prng::Prng;
pub use render::{
    coverage_from_shape, parse_background, shape_coverage, shape_outline, tile_from_buffer,
    RegionRender, RenderOptions, RenderStats, Renderer, MAX_EFFECT_PADDING, RAW_RGBA_MIME,
};
pub use thumb::{render_thumbnail, Thumb, ThumbKind, ThumbStats, THUMB_BLOCK};
pub use tile::{
    Tile, TileCache, TileCacheStats, TileGrid, TileKey, ALLOWED_TILE_SIZES, DEFAULT_TILE_SIZE,
};
