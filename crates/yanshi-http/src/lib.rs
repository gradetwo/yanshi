//! # 偃师 Yanshi 零依赖 HTTP/1.1 与 WebSocket 传输层
//!
//! 只依赖 `std::net` / `std::io` 实现（HTTP 解析、WebSocket 握手与帧、线程模型），
//! 把 [`yanshi_server`] 的工具协议层暴露给 Web 查看器与 HTTP 客户端。
//!
//! | 模块 | 设计文档 | 职责 |
//! |---|---|---|
//! | [`sha1`] | 3 章握手 | 手写 SHA-1（WebSocket `Sec-WebSocket-Accept`） |
//! | [`ws`] | 6.8 / 12.8 | RFC 6455 握手、帧编解码、分片、ping/pong/close |
//! | [`http`] | 10 章 | HTTP/1.1 请求解析、查询解码、响应与 5.7 状态码映射 |
//! | [`server`] | 3 / 6.8 / 12.7 | 路由、capability token 鉴权、WS 会话与推送 |
//! | [`viewer`] | 6.2 / 7 章 | 最小 Web 查看器（单页 HTML/JS，无前端依赖） |

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// **★ GPU 分流策略（**第 281 轮 ✓）★**
pub mod gpu_policy;
pub mod http;
pub mod server;
pub mod sha1;
pub mod viewer;
pub mod ws;

pub use http::{Method, Request, Response, MAX_BODY_BYTES};
pub use server::{serve, HttpOptions, ServerHandle, ServerState};
pub use sha1::{sha1, Sha1};
pub use ws::{
    accept_key, is_valid_client_key, read_frame, read_message, write_frame, Frame, OpCode,
};
