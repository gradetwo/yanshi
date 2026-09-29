//! 确定性 PRNG（设计文档 6.1 / 11.1）。
//!
//! 随机纹理、抖动由原子 `seed` 控制，可重放；不使用系统时钟或线程调度，
//! 因此客户端与服务端、不同平台上的同一份历史得到相同像素。

/// splitmix64 驱动的确定性随机源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prng {
    state: u64,
}

impl Prng {
    /// 以 seed 构造。
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// 由种子与序号派生一个独立的随机源（用于每个 stamp 的抖动）。
    pub fn derive(seed: u64, index: u64) -> Self {
        Self::new(seed ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }

    /// 下一个 64 位随机数。
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `[0, 1)` 均匀分布。
    pub fn unit(&mut self) -> f32 {
        // 取高 24 位，保证尾数精度。
        ((self.next_u64() >> 40) as f32) / (1u32 << 24) as f32
    }

    /// `[-1, 1)` 均匀分布。
    pub fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }

    /// `[low, high)` 均匀分布。
    pub fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Prng::new(7);
        let mut b = Prng::new(7);
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        let mut c = Prng::new(8);
        assert_ne!(a.unit(), c.unit(), "不同 seed 应给出不同序列");
    }

    #[test]
    fn derived_streams_are_independent_but_reproducible() {
        let first: Vec<f32> = (0..32).map(|i| Prng::derive(42, i).unit()).collect();
        let again: Vec<f32> = (0..32).map(|i| Prng::derive(42, i).unit()).collect();
        assert_eq!(first, again);
        assert!(first.windows(2).all(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn range_and_signed_respect_bounds() {
        let mut rng = Prng::new(1);
        for _ in 0..1000 {
            let unit = rng.unit();
            assert!((0.0..1.0).contains(&unit), "unit={unit}");
            let signed = rng.signed();
            assert!((-1.0..1.0).contains(&signed), "signed={signed}");
            let ranged = rng.range(2.0, 5.0);
            assert!((2.0..5.0).contains(&ranged), "ranged={ranged}");
        }
    }
}
