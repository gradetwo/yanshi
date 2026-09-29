//! RFC 6455 WebSocket 握手与帧编解码（零依赖）。
//!
//! 只用 `std::net` / `std::io`：握手用 [`crate::sha1`] + base64，
//! 帧支持 7/16/64 位长度、客户端掩码、分片重组、ping/pong/close。

use crate::sha1::sha1;
use std::io::{self, Read, Write};

/// 握手用的固定 GUID（RFC 6455 §1.3）。
pub const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
/// 单帧与单条消息的默认上限（16 MiB）——超过即视为协议错误，防止内存被拖垮。
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// WebSocket 操作码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCode {
    /// 分片续帧。
    Continuation,
    /// 文本帧。
    Text,
    /// 二进制帧。
    Binary,
    /// 关闭。
    Close,
    /// Ping。
    Ping,
    /// Pong。
    Pong,
}

impl OpCode {
    /// 线上取值。
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Continuation => 0x0,
            Self::Text => 0x1,
            Self::Binary => 0x2,
            Self::Close => 0x8,
            Self::Ping => 0x9,
            Self::Pong => 0xA,
        }
    }

    /// 由线上取值解析（保留位或未知操作码返回 `None`）。
    pub const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0x0 => Self::Continuation,
            0x1 => Self::Text,
            0x2 => Self::Binary,
            0x8 => Self::Close,
            0x9 => Self::Ping,
            0xA => Self::Pong,
            _ => return None,
        })
    }

    /// 是否是控制帧（控制帧不可分片，且必须立即处理）。
    pub const fn is_control(self) -> bool {
        matches!(self, Self::Close | Self::Ping | Self::Pong)
    }
}

/// 一帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// 是否末帧。
    pub fin: bool,
    /// 操作码。
    pub opcode: OpCode,
    /// 载荷。
    pub payload: Vec<u8>,
}

impl Frame {
    /// 文本帧。
    pub fn text(payload: impl Into<String>) -> Self {
        Self {
            fin: true,
            opcode: OpCode::Text,
            payload: payload.into().into_bytes(),
        }
    }

    /// 控制帧。
    pub fn control(opcode: OpCode, payload: Vec<u8>) -> Self {
        Self {
            fin: true,
            opcode,
            payload,
        }
    }

    /// 关闭帧（可带状态码）。
    pub fn close(code: u16) -> Self {
        Self::control(OpCode::Close, code.to_be_bytes().to_vec())
    }
}

/// 计算 `Sec-WebSocket-Accept`。
pub fn accept_key(client_key: &str) -> String {
    let digest = sha1(format!("{client_key}{WS_GUID}").as_bytes());
    yanshi_server::base64::encode(&digest)
}

/// 校验客户端 `Sec-WebSocket-Key`（base64 的 16 字节 = 24 字符）。
pub fn is_valid_client_key(key: &str) -> bool {
    key.len() == 24
        && key.ends_with("==")
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
}

/// 编码一帧；服务端发往客户端的帧**不加掩码**。
pub fn encode_frame(opcode: OpCode, fin: bool, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 10);
    out.push((if fin { 0x80 } else { 0x00 }) | opcode.as_u8());
    let length = payload.len();
    if length < 126 {
        out.push(length as u8);
    } else if length <= u16::MAX as usize {
        out.push(126);
        out.extend_from_slice(&(length as u16).to_be_bytes());
    } else {
        out.push(127);
        out.extend_from_slice(&(length as u64).to_be_bytes());
    }
    out.extend_from_slice(payload);
    out
}

/// 编码客户端帧（带掩码）。服务端与测试客户端都可以用它构造合法客户端帧。
pub fn encode_client_frame(opcode: OpCode, fin: bool, payload: &[u8], mask: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 14);
    out.push((if fin { 0x80 } else { 0x00 }) | opcode.as_u8());
    let length = payload.len();
    if length < 126 {
        out.push(0x80 | length as u8);
    } else if length <= u16::MAX as usize {
        out.push(0x80 | 126);
        out.extend_from_slice(&(length as u16).to_be_bytes());
    } else {
        out.push(0x80 | 127);
        out.extend_from_slice(&(length as u64).to_be_bytes());
    }
    out.extend_from_slice(&mask);
    for (index, byte) in payload.iter().enumerate() {
        out.push(byte ^ mask[index % 4]);
    }
    out
}

/// 读取一帧，**不要求掩码**（WebSocket 客户端与测试用）。
pub fn read_frame_any<R: Read>(reader: &mut R) -> io::Result<Option<Frame>> {
    read_frame_inner(reader, false)
}

/// 读取一帧；流结束（EOF）返回 `Ok(None)`。
///
/// 客户端到服务端的帧必须带掩码（RFC 6455 §5.1），否则报 `InvalidData`。
pub fn read_frame<R: Read>(reader: &mut R) -> io::Result<Option<Frame>> {
    read_frame_inner(reader, true)
}

fn read_frame_inner<R: Read>(reader: &mut R, require_mask: bool) -> io::Result<Option<Frame>> {
    let mut header = [0u8; 2];
    match reader.read_exact(&mut header) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let fin = header[0] & 0x80 != 0;
    if header[0] & 0x70 != 0 {
        return Err(invalid("帧使用了保留位（未协商扩展）"));
    }
    let opcode =
        OpCode::from_u8(header[0] & 0x0F).ok_or_else(|| invalid("未知 WebSocket 操作码"))?;
    let masked = header[1] & 0x80 != 0;
    let length_code = header[1] & 0x7F;

    let length = match length_code {
        126 => {
            let mut buffer = [0u8; 2];
            reader.read_exact(&mut buffer)?;
            u16::from_be_bytes(buffer) as usize
        }
        127 => {
            let mut buffer = [0u8; 8];
            reader.read_exact(&mut buffer)?;
            let value = u64::from_be_bytes(buffer);
            if value > MAX_FRAME_BYTES as u64 {
                return Err(invalid("WebSocket 帧超过上限"));
            }
            value as usize
        }
        other => other as usize,
    };
    if length > MAX_FRAME_BYTES {
        return Err(invalid("WebSocket 帧超过上限"));
    }
    if opcode.is_control() && (!fin || length > 125) {
        return Err(invalid("控制帧必须不分片且载荷 ≤ 125 字节"));
    }
    if require_mask && !masked {
        return Err(invalid("客户端帧必须带掩码"));
    }

    let mut mask = [0u8; 4];
    if masked {
        reader.read_exact(&mut mask)?;
    }
    let mut payload = vec![0u8; length];
    reader.read_exact(&mut payload)?;
    if masked {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }
    }
    Ok(Some(Frame {
        fin,
        opcode,
        payload,
    }))
}

/// 写入一帧（服务端 → 客户端）。
pub fn write_frame<W: Write>(writer: &mut W, frame: &Frame) -> io::Result<()> {
    let bytes = encode_frame(frame.opcode, frame.fin, &frame.payload);
    writer.write_all(&bytes)?;
    writer.flush()
}

/// 读取一条完整消息（自动重组分片、自动回 pong），返回 `(opcode, payload)`。
///
/// 返回 `Ok(None)` 表示对端关闭了连接（收到 close 或 EOF）。
pub fn read_message<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> io::Result<Option<(OpCode, Vec<u8>)>> {
    let mut assembled: Option<(OpCode, Vec<u8>)> = None;
    loop {
        let Some(frame) = read_frame(reader)? else {
            return Ok(None);
        };
        match frame.opcode {
            OpCode::Ping => {
                write_frame(writer, &Frame::control(OpCode::Pong, frame.payload))?;
                continue;
            }
            OpCode::Pong => continue,
            OpCode::Close => {
                // 回一个 close 后结束。
                let _ = write_frame(writer, &Frame::close(1000));
                return Ok(None);
            }
            OpCode::Continuation => match &mut assembled {
                Some((_, buffer)) => {
                    if buffer.len() + frame.payload.len() > MAX_FRAME_BYTES {
                        return Err(invalid("WebSocket 消息超过上限"));
                    }
                    buffer.extend_from_slice(&frame.payload);
                    if frame.fin {
                        let (opcode, payload) = assembled.take().expect("已判定为 Some");
                        return Ok(Some((opcode, payload)));
                    }
                }
                None => return Err(invalid("收到无起始帧的续帧")),
            },
            opcode => {
                if assembled.is_some() {
                    return Err(invalid("分片未结束时又收到新的数据帧"));
                }
                if frame.fin {
                    return Ok(Some((opcode, frame.payload)));
                }
                assembled = Some((opcode, frame.payload));
            }
        }
    }
}

/// 把消息切片成分片帧序列（服务端发送大消息时使用）。
pub fn split_message(opcode: OpCode, payload: &[u8], chunk: usize) -> Vec<Frame> {
    let chunk = chunk.clamp(1, MAX_FRAME_BYTES);
    if payload.len() <= chunk {
        return vec![Frame {
            fin: true,
            opcode,
            payload: payload.to_vec(),
        }];
    }
    let mut frames = Vec::new();
    let mut offset = 0usize;
    let mut first = true;
    while offset < payload.len() {
        let end = (offset + chunk).min(payload.len());
        frames.push(Frame {
            fin: end == payload.len(),
            opcode: if first { opcode } else { OpCode::Continuation },
            payload: payload[offset..end].to_vec(),
        });
        first = false;
        offset = end;
    }
    frames
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// RFC 6455 §1.3 的握手示例。
    #[test]
    fn accept_key_matches_rfc_example() {
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
        assert!(is_valid_client_key("dGhlIHNhbXBsZSBub25jZQ=="));
        assert!(!is_valid_client_key("short"));
        assert!(!is_valid_client_key("dGhlIHNhbXBsZSBub25jZQ="));
    }

    #[test]
    fn frame_round_trip_with_masking() {
        for payload in [vec![], b"hello".to_vec(), vec![7u8; 300], vec![9u8; 70_000]] {
            let frame = Frame {
                fin: true,
                opcode: OpCode::Text,
                payload: payload.clone(),
            };
            let encoded = encode_frame(frame.opcode, frame.fin, &payload);
            // 手工给服务端编码的帧加上客户端掩码，模拟客户端发送。
            let masked = mask_frame(&encoded, [0x12, 0x34, 0x56, 0x78]);
            let decoded = read_frame(&mut Cursor::new(masked)).unwrap().unwrap();
            assert_eq!(decoded, frame, "载荷长度 {}", payload.len());
        }
    }

    fn mask_frame(server_frame: &[u8], mask: [u8; 4]) -> Vec<u8> {
        // 解析服务端帧（无掩码）后按客户端规范重新编码。
        let mut cursor = Cursor::new(server_frame);
        let mut header = [0u8; 2];
        cursor.read_exact(&mut header).unwrap();
        let length_code = header[1] & 0x7F;
        let length = match length_code {
            126 => {
                let mut buffer = [0u8; 2];
                cursor.read_exact(&mut buffer).unwrap();
                u16::from_be_bytes(buffer) as usize
            }
            127 => {
                let mut buffer = [0u8; 8];
                cursor.read_exact(&mut buffer).unwrap();
                u64::from_be_bytes(buffer) as usize
            }
            other => other as usize,
        };
        let mut payload = vec![0u8; length];
        cursor.read_exact(&mut payload).unwrap();
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }
        let mut out = Vec::new();
        out.push(header[0]);
        match length {
            value if value < 126 => out.push(0x80 | value as u8),
            value if value <= u16::MAX as usize => {
                out.push(0x80 | 126);
                out.extend_from_slice(&(value as u16).to_be_bytes());
            }
            value => {
                out.push(0x80 | 127);
                out.extend_from_slice(&(value as u64).to_be_bytes());
            }
        }
        out.extend_from_slice(&mask);
        out.extend_from_slice(&payload);
        out
    }

    #[test]
    fn client_frame_helper_is_accepted_by_the_server_reader() {
        let masked = encode_client_frame(OpCode::Text, true, b"hello", [1, 2, 3, 4]);
        let frame = read_frame(&mut Cursor::new(masked.clone()))
            .unwrap()
            .unwrap();
        assert_eq!(frame, Frame::text("hello"));
        // 客户端读取服务端帧时不要求掩码。
        let server_frame = encode_frame(OpCode::Text, true, b"hi");
        assert_eq!(
            read_frame_any(&mut Cursor::new(server_frame))
                .unwrap()
                .unwrap(),
            Frame::text("hi")
        );
        // 掩码帧同样可被 read_frame_any 读出。
        assert_eq!(
            read_frame_any(&mut Cursor::new(masked)).unwrap().unwrap(),
            Frame::text("hello")
        );
    }

    #[test]
    fn unmasked_client_frame_is_rejected() {
        let encoded = encode_frame(OpCode::Text, true, b"hi");
        let error = read_frame(&mut Cursor::new(encoded)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn reserved_bits_and_unknown_opcodes_are_rejected() {
        let mut frame = vec![0x80 | 0x1, 0x80, 0, 0, 0, 0];
        frame[0] |= 0x40; // RSV1
        assert!(read_frame(&mut Cursor::new(frame)).is_err());

        let mut frame = vec![0x80 | 0x3, 0x80, 0, 0, 0, 0];
        frame[0] = 0x80 | 0x3; // 保留操作码
        assert!(read_frame(&mut Cursor::new(frame)).is_err());
    }

    #[test]
    fn ping_is_answered_with_pong_and_close_ends_stream() {
        let mut input = Vec::new();
        input.extend_from_slice(&mask_frame(
            &encode_frame(OpCode::Ping, true, b"hb"),
            [1, 2, 3, 4],
        ));
        input.extend_from_slice(&mask_frame(
            &encode_frame(OpCode::Text, true, b"after"),
            [1, 2, 3, 4],
        ));
        let mut output = Vec::new();
        let message = read_message(&mut Cursor::new(input), &mut output)
            .unwrap()
            .unwrap();
        assert_eq!(message, (OpCode::Text, b"after".to_vec()));
        assert_eq!(
            output,
            encode_frame(OpCode::Pong, true, b"hb"),
            "自动回 pong"
        );

        let mut close_input = Vec::new();
        close_input.extend_from_slice(&mask_frame(
            &encode_frame(OpCode::Close, true, &1000u16.to_be_bytes()),
            [1, 2, 3, 4],
        ));
        let mut close_output = Vec::new();
        assert!(
            read_message(&mut Cursor::new(close_input), &mut close_output)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            close_output,
            encode_frame(OpCode::Close, true, &1000u16.to_be_bytes())
        );
    }

    #[test]
    fn fragmented_message_is_reassembled() {
        let mut input = Vec::new();
        input.extend_from_slice(&mask_frame(
            &encode_frame(OpCode::Text, false, b"he"),
            [9, 9, 9, 9],
        ));
        input.extend_from_slice(&mask_frame(
            &encode_frame(OpCode::Continuation, false, b"ll"),
            [9, 9, 9, 9],
        ));
        input.extend_from_slice(&mask_frame(
            &encode_frame(OpCode::Continuation, true, b"o"),
            [9, 9, 9, 9],
        ));
        let mut output = Vec::new();
        let message = read_message(&mut Cursor::new(input), &mut output)
            .unwrap()
            .unwrap();
        assert_eq!(message, (OpCode::Text, b"hello".to_vec()));
        assert!(output.is_empty());

        // 无起始帧的续帧是协议错误。
        let mut bad = Vec::new();
        bad.extend_from_slice(&mask_frame(
            &encode_frame(OpCode::Continuation, true, b"x"),
            [0, 0, 0, 0],
        ));
        assert!(read_message(&mut Cursor::new(bad), &mut Vec::new()).is_err());
    }

    #[test]
    fn control_frame_rules_are_enforced() {
        // 控制帧不可分片。
        let mut frame = encode_frame(OpCode::Ping, true, b"x");
        frame[0] &= 0x7F; // 清掉 FIN
        assert!(read_frame(&mut Cursor::new(frame)).is_err());
        // 控制帧载荷 ≤ 125。
        let mut engine = Vec::new();
        engine.push(0x80 | 0x9);
        engine.push(0x80 | 126);
        engine.extend_from_slice(&200u16.to_be_bytes());
        engine.extend_from_slice(&[0, 0, 0, 0]);
        engine.extend_from_slice(&[0u8; 200]);
        assert!(read_frame(&mut Cursor::new(engine)).is_err());
    }

    #[test]
    fn oversize_frames_are_rejected() {
        let mut engine = Vec::new();
        engine.push(0x80 | 0x2);
        engine.push(0x80 | 127);
        engine.extend_from_slice(&((MAX_FRAME_BYTES as u64) + 1).to_be_bytes());
        let error = read_frame(&mut Cursor::new(engine)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn split_message_produces_valid_fragments() {
        let payload = vec![3u8; 1000];
        let frames = split_message(OpCode::Binary, &payload, 300);
        assert_eq!(frames.len(), 4);
        assert_eq!(frames[0].opcode, OpCode::Binary);
        assert!(!frames[0].fin);
        assert_eq!(frames[3].opcode, OpCode::Continuation);
        assert!(frames[3].fin);
        let total: Vec<u8> = frames
            .iter()
            .flat_map(|frame| frame.payload.clone())
            .collect();
        assert_eq!(total, payload);
    }
}
