//! # 偃师 Yanshi 渲染计算内核层
//!
//! 本 crate 是设计文档 v1.0-draft4 第 6.1 节定义的**计算内核层**参考实现：
//! 纯 CPU、无 GPU 依赖、确定性（D0 bit-exact 基线），客户端与服务端共享同一份代码。
//!
//! | 模块 | 设计文档 | 职责 |
//! |---|---|---|
//! | [`half`] | 6.1 | IEEE-754 binary16 存储（内存 tile 用 f16 线性） |
//! | [`color`] | 6.1 | 线性光空间、预乘 alpha、sRGB 传递函数、u8 打包 |
//! | [`blend`] | 6.1 | 混合模式（W3C 合成公式，预乘 alpha） |
//! | [`prng`] | 6.1 / 11.1 | 由 seed 驱动的确定性 PRNG（抖动、纹理） |
//! | [`geometry`] | 8.1 | 包围盒、形状覆盖率光栅化（矩形/椭圆/多边形） |
//! | [`tile`] | 6.4 / 13.3 | Tile、TileGrid、TileCache（LRU、显存预算、视口淘汰） |

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod blend;
pub mod color;
pub mod geometry;
pub mod half;
pub mod prng;
pub mod tile;

pub use blend::{blend_pixel, over, BlendMode};
pub use color::{
    linear_premul_to_u8x4, linear_to_srgb, srgb_to_linear, u8x4_to_linear_premul, LinearRgba,
};
pub use geometry::{
    dashed_line, ellipse_coverage, point_in_polygon, polygon_coverage, rect_coverage, Coverage,
};
pub use half::{f16_bits_to_f32, f32_to_f16_bits};
pub use prng::Prng;
pub use tile::{
    Tile, TileCache, TileCacheStats, TileGrid, TileKey, ALLOWED_TILE_SIZES, DEFAULT_TILE_SIZE,
};
