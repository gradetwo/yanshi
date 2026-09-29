//! SHA-1（FIPS 180-1 / RFC 3174）。
//!
//! WebSocket 握手需要 `SHA1(key + GUID)` 再 base64。整个仓库坚持零网络/加密依赖，
//! 因此这里手写一份**仅用于握手**的 SHA-1 实现（不用于签名或密码学安全场景；
//! 17 章远期项会用 Ed25519 做签名）。

/// SHA-1 摘要长度。
pub const DIGEST_LEN: usize = 20;

/// SHA-1 流式哈希。
#[derive(Debug, Clone)]
pub struct Sha1 {
    state: [u32; 5],
    buffer: [u8; 64],
    buffered: usize,
    length_bits: u64,
}

impl Default for Sha1 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha1 {
    /// 初始状态。
    pub const fn new() -> Self {
        Self {
            state: [
                0x6745_2301,
                0xEFCD_AB89,
                0x98BA_DCFE,
                0x1032_5476,
                0xC3D2_E1F0,
            ],
            buffer: [0u8; 64],
            buffered: 0,
            length_bits: 0,
        }
    }

    /// 追加数据。
    pub fn update(&mut self, mut data: &[u8]) {
        self.length_bits = self.length_bits.wrapping_add((data.len() as u64) * 8);
        if self.buffered > 0 {
            let need = 64 - self.buffered;
            let take = need.min(data.len());
            self.buffer[self.buffered..self.buffered + take].copy_from_slice(&data[..take]);
            self.buffered += take;
            data = &data[take..];
            if self.buffered == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffered = 0;
            }
        }
        let mut chunks = data.chunks_exact(64);
        for chunk in &mut chunks {
            let mut block = [0u8; 64];
            block.copy_from_slice(chunk);
            self.compress(&block);
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            self.buffer[..rest.len()].copy_from_slice(rest);
            self.buffered = rest.len();
        }
    }

    /// 完成并输出 20 字节摘要。
    pub fn finalize(mut self) -> [u8; DIGEST_LEN] {
        let length_bits = self.length_bits;
        // padding: 0x80 后补零到 56 mod 64，再附 64 位大端长度。
        self.update_raw(&[0x80]);
        while self.buffered != 56 {
            self.update_raw(&[0x00]);
        }
        self.update_raw(&length_bits.to_be_bytes());
        let mut out = [0u8; DIGEST_LEN];
        for (index, word) in self.state.iter().enumerate() {
            out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    /// 只写缓冲区（不改长度计数），供 padding 使用。
    fn update_raw(&mut self, data: &[u8]) {
        for byte in data {
            self.buffer[self.buffered] = *byte;
            self.buffered += 1;
            if self.buffered == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffered = 0;
            }
        }
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 80];
        for index in 0..16 {
            w[index] = u32::from_be_bytes([
                block[index * 4],
                block[index * 4 + 1],
                block[index * 4 + 2],
                block[index * 4 + 3],
            ]);
        }
        for index in 16..80 {
            w[index] = (w[index - 3] ^ w[index - 8] ^ w[index - 14] ^ w[index - 16]).rotate_left(1);
        }

        let [mut a, mut b, mut c, mut d, mut e] = self.state;
        for (index, word) in w.iter().enumerate() {
            let (f, k) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
    }
}

/// 一次性计算 SHA-1。
pub fn sha1(data: &[u8]) -> [u8; DIGEST_LEN] {
    let mut hasher = Sha1::new();
    hasher.update(data);
    hasher.finalize()
}

/// 十六进制编码（调试与测试）。
pub fn to_hex(digest: &[u8; DIGEST_LEN]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3174_vectors() {
        assert_eq!(
            to_hex(&sha1(b"")),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        assert_eq!(
            to_hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            to_hex(&sha1(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        assert_eq!(
            to_hex(&sha1(
                b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"
            )),
            "a49b2446a02c645bf419f995b67091253a04a259"
        );
    }

    #[test]
    fn long_input_streaming_matches_one_shot() {
        let data = vec![b'a'; 1_000_000];
        assert_eq!(
            to_hex(&sha1(&data)),
            "34aa973cd4c4daa4f61eeb2bdbad27316534016f",
            "一百万个 'a'"
        );

        // 分块喂入与一次性结果一致（覆盖 64 字节边界与跨块拼接）。
        for chunk in [1usize, 7, 63, 64, 65, 127, 128, 1000] {
            let mut hasher = Sha1::new();
            for piece in data.chunks(chunk) {
                hasher.update(piece);
            }
            assert_eq!(hasher.finalize(), sha1(&data), "分块 {chunk}");
        }
    }

    #[test]
    fn padding_boundaries_are_handled() {
        // 55、56、64 字节边界正好落在 padding 分界上。
        for length in [54usize, 55, 56, 57, 63, 64, 65, 119, 120, 121] {
            let data = vec![0x5Au8; length];
            let mut hasher = Sha1::new();
            hasher.update(&data);
            assert_eq!(hasher.finalize(), sha1(&data), "长度 {length}");
        }
    }
}
