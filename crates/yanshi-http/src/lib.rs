//! # 偃师 Yanshi 零依赖 HTTP/1.1 与 WebSocket 传输层
//!
//! 只依赖 `std::net` / `std::io` 实现（HTTP 解析、WebSocket 握手与帧、线程模型），
//! 把 [`yanshi_server`] 的工具协议层暴露给 Web 查看器与 HTTP 客户端。
//!
//! | 模块 | 设计文档 | 职责 |
//! |---|---|---|
//! | [`sha1`] | 3 章握手 | 手写 SHA-1（WebSocket `Sec-WebSocket-Accept`） |
//! | [`ws`] | 6.8 / 12.8 | RFC 6455 握手、帧编解码、分片、ping/pong/close |

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod sha1;
pub mod ws;

pub use sha1::{sha1, Sha1};
pub use ws::{
    accept_key, is_valid_client_key, read_frame, read_message, write_frame, Frame, OpCode,
};
