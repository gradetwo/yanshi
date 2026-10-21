//! Base64 编解码（MCP `image` content 需要编码；文件双通道的
//! `data_base64` 入参需要解码——设计文档第 4 章落地后服务端也收了）。
//!
//! 只实现标准字母表 + `=` 填充；解码是严格式：长度必须是 4 的倍数、
//! `=` 只许在末尾（至多 2 个）、正文不许混其它字符——含糊的输入一律拒收
//! 并由调用方翻译成参数错误（不静默容忍空白或 URL-safe 字母表）。

/// 标准 base64 字母表。
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// 编码为 base64（带填充）。
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// 解码标准 base64（带 `=` 填充）。任何不合格式的输入返回 `None`。
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let raw = text.as_bytes();
    if raw.len() % 4 != 0 {
        return None;
    }
    let pads = raw.iter().rev().take_while(|&&c| c == b'=').count();
    if pads > 2 {
        return None;
    }
    let body = &raw[..raw.len() - pads];
    if body.contains(&b'=') {
        return None;
    }
    // 结构一致性：正文长度 mod 4 只能是 0（无填充）、2（配 2 个 `=`）、
    // 3（配 1 个 `=`）；余 1 无论如何凑不出整字节，直接拒。
    match (body.len() % 4, pads) {
        (0, 0) | (2, 2) | (3, 1) => {}
        _ => return None,
    }
    let mut table = [u8::MAX; 256];
    for (value, &c) in ALPHABET.iter().enumerate() {
        table[c as usize] = value as u8;
    }
    let mut out = Vec::with_capacity(body.len() / 4 * 3 + 3);
    let mut acc: u32 = 0;
    let mut nbits: u32 = 0;
    for &c in body {
        let sextet = table[c as usize];
        if sextet == u8::MAX {
            return None;
        }
        acc = (acc << 6) | u32::from(sextet);
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((acc >> nbits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_rfc4648_vectors() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
        assert_eq!(encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn decode_matches_rfc4648_vectors() {
        assert_eq!(decode(""), Some(vec![]));
        assert_eq!(decode("Zg=="), Some(b"f".to_vec()));
        assert_eq!(decode("Zm8="), Some(b"fo".to_vec()));
        assert_eq!(decode("Zm9v"), Some(b"foo".to_vec()));
        assert_eq!(decode("Zm9vYg=="), Some(b"foob".to_vec()));
        assert_eq!(decode("Zm9vYmE="), Some(b"fooba".to_vec()));
        assert_eq!(decode("Zm9vYmFy"), Some(b"foobar".to_vec()));
    }

    #[test]
    fn decode_rejects_malformed_input() {
        assert_eq!(decode("Zg="), None, "长度不是 4 的倍数");
        assert_eq!(decode("Z==="), None, "填充超过 2 个");
        assert_eq!(decode("Zg==Zg=="), None, "`=` 混在正文里");
        assert_eq!(decode("Zm9v Yg=="), None, "空白不容忍");
        assert_eq!(decode("Zm9vYg"), None, "缺填充的截断正文");
        assert_eq!(decode("Zm9-"), None, "URL-safe 字母表不收");
        assert_eq!(decode("ZZZZZ==="), None, "填充与正文长度对不上");
    }

    #[test]
    fn encode_then_decode_round_trips_binary() {
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(decode(&encode(&bytes)), Some(bytes));
    }

    #[test]
    fn binary_payload_round_trips_length() {
        let bytes: Vec<u8> = (0..=255).collect();
        let encoded = encode(&bytes);
        assert_eq!(encoded.len(), bytes.len().div_ceil(3) * 4);
        assert!(encoded
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'='));
    }
}
