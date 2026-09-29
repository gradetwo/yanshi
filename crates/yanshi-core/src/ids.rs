//! 标识与序号（设计文档 5.1）。
//!
//! 原子 `id` 由客户端生成、服务端按 id 去重，因此客户端标识必须**全局唯一且可排序**；
//! 权威排序由服务端分配的 [`Seq`] 承担，ULID 只用于幂等去重与审计。

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// 服务端权威序号：折叠的唯一排序依据（设计文档 5.1）。
pub type Seq = u64;

/// 原子标识（客户端生成的 ULID 字符串）。
pub type AtomId = String;

/// 文档标识。
pub type DocId = String;
/// 图层标识。
pub type LayerId = String;
/// 对象标识。
pub type ObjectId = String;
/// 选区标识。
pub type SelectionId = String;
/// 蒙版标识。
pub type MaskId = String;
/// 风格标识。
pub type StyleId = String;
/// 检查点标识。
pub type CheckpointId = String;
/// 快照标识。
pub type SnapshotId = String;
/// 变更集标识。
pub type ChangesetId = String;
/// 会话标识。
pub type SessionId = String;
/// 操作者标识（`actor`）。
pub type ActorId = String;

/// Crockford Base32 字母表（ULID 规范，排除 I/L/O/U）。
const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// ULID 编码长度：128 bit / 5 bit 向上取整。
pub const ULID_LEN: usize = 26;
/// 时间戳位数（毫秒）。
const TS_BITS: u32 = 48;
/// 随机位宽。
const ENTROPY_BITS: u32 = 80;

/// 进程级随机种子，用于构造 entropy 高位。
static PROCESS_SEED: OnceLock<u64> = OnceLock::new();
/// 同一毫秒内的单调计数器。
static COUNTER: AtomicU64 = AtomicU64::new(0);

fn process_seed() -> u64 {
    *PROCESS_SEED.get_or_init(|| {
        // RandomState 由 OS 熵源播种，无需额外依赖。
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0),
        );
        hasher.finish()
    })
}

/// 当前 Unix 毫秒时间戳。
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 通用唯一字典序可排序标识符（ULID，设计文档 5.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Ulid {
    ts_ms: u64,
    entropy: u128,
}

impl Ulid {
    /// 由毫秒时间戳与 80 bit entropy 构造（entropy 高位溢出被截断）。
    pub fn from_parts(ts_ms: u64, entropy: u128) -> Self {
        Self {
            ts_ms: ts_ms & ((1u64 << TS_BITS) - 1),
            entropy: entropy & ((1u128 << ENTROPY_BITS) - 1),
        }
    }

    /// 生成新 ULID：时间戳 + 进程随机种子 + 进程内单调计数器。
    ///
    /// 同一进程内同一毫秒最多 65536 个标识；跨进程由随机种子区分。
    /// 时间戳回拨时不保证字典序单调，但唯一性由种子与计数器保证。
    pub fn new() -> Self {
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let entropy = ((process_seed() as u128) << 16) | u128::from(counter & 0xFFFF);
        Self::from_parts(now_ms() as u64, entropy)
    }

    /// 毫秒时间戳部分。
    pub const fn timestamp_ms(self) -> u64 {
        self.ts_ms
    }

    /// 80 bit 随机部分。
    pub const fn entropy(self) -> u128 {
        self.entropy
    }

    /// 128 bit 数值形式。
    pub const fn to_u128(self) -> u128 {
        ((self.ts_ms as u128) << ENTROPY_BITS) | self.entropy
    }

    /// 编码为 26 字符 Crockford Base32。
    pub fn encode(self) -> String {
        let mut value = self.to_u128();
        let mut buf = [0u8; ULID_LEN];
        for slot in buf.iter_mut().rev() {
            *slot = CROCKFORD[(value & 0x1F) as usize];
            value >>= 5;
        }
        String::from_utf8(buf.to_vec()).expect("Crockford 字母表为 ASCII")
    }
}

impl Default for Ulid {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Ulid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.encode())
    }
}

impl FromStr for Ulid {
    type Err = UlidParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != ULID_LEN {
            return Err(UlidParseError::Length(s.len()));
        }
        let mut value: u128 = 0;
        for (idx, ch) in s.bytes().enumerate() {
            let ch = ch.to_ascii_uppercase();
            let digit = CROCKFORD
                .iter()
                .position(|c| *c == ch)
                .ok_or(UlidParseError::Character(ch as char))?;
            if idx == 0 && digit > 7 {
                return Err(UlidParseError::Overflow);
            }
            value = (value << 5) | digit as u128;
        }
        Ok(Self::from_parts(
            (value >> ENTROPY_BITS) as u64,
            value & ((1u128 << ENTROPY_BITS) - 1),
        ))
    }
}

impl TryFrom<String> for Ulid {
    type Error = UlidParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Ulid> for String {
    fn from(value: Ulid) -> Self {
        value.encode()
    }
}

/// ULID 解析错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UlidParseError {
    /// 长度不是 26。
    #[error("ULID 长度必须为 {ULID_LEN}，实际为 {0}")]
    Length(usize),
    /// 出现非法字符。
    #[error("ULID 含非法字符 {0:?}")]
    Character(char),
    /// 首字符超出 3 bit 取值。
    #[error("ULID 首字符溢出")]
    Overflow,
}

/// 可确定性复现的标识生成器（测试、回放与 fuzz 使用）。
///
/// 服务端不需要它；它存在的目的是让属性测试在给定种子下完全可复现。
#[derive(Debug, Clone)]
pub struct UlidGen {
    next: u64,
    seed: u64,
    ts_ms: u64,
}

impl UlidGen {
    /// 用固定种子构造（默认时间戳为 0，保证跨机器一致）。
    pub fn seeded(seed: u64) -> Self {
        Self {
            next: 0,
            seed,
            ts_ms: 1_700_000_000_000,
        }
    }

    /// 生成下一个标识。
    pub fn next_ulid(&mut self) -> Ulid {
        let n = self.next;
        self.next += 1;
        let mixed = splitmix64(self.seed ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        Ulid::from_parts(self.ts_ms + n / 1024, u128::from(mixed))
    }

    /// 生成下一个标识的字符串形式。
    pub fn next_id(&mut self) -> String {
        self.next_ulid().encode()
    }
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let cases = [
            Ulid::from_parts(0, 0),
            Ulid::from_parts(1, 1),
            Ulid::from_parts(u64::MAX, u128::MAX),
            Ulid::from_parts(1_700_000_000_000, 0x0123_4567_89AB_CDEF_0123),
        ];
        for ulid in cases {
            let text = ulid.encode();
            assert_eq!(text.len(), ULID_LEN);
            assert_eq!(text.parse::<Ulid>().unwrap(), ulid);
        }
    }

    #[test]
    fn lexicographic_order_matches_timestamp_order() {
        let a = Ulid::from_parts(1, 0);
        let b = Ulid::from_parts(2, 0);
        assert!(a.encode() < b.encode());
        assert!(a < b);
    }

    #[test]
    fn new_is_unique_and_monotonic_within_process() {
        let mut seen = std::collections::BTreeSet::new();
        let mut prev = Ulid::new();
        for _ in 0..10_000 {
            let next = Ulid::new();
            assert!(seen.insert(next));
            assert!(next >= prev, "同一进程内 ULID 不应回退");
            prev = next;
        }
    }

    #[test]
    fn rejects_bad_input() {
        assert!(matches!(
            "TOO_SHORT".parse::<Ulid>(),
            Err(UlidParseError::Length(9))
        ));
        assert!(matches!(
            "01ARZ3NDEKTSV4RRFFQ69G5FAI".parse::<Ulid>(),
            Err(UlidParseError::Character('I'))
        ));
        // 首字符只能取 0..7。
        assert!(matches!(
            "Z1ARZ3NDEKTSV4RRFFQ69G5FAV".parse::<Ulid>(),
            Err(UlidParseError::Overflow)
        ));
    }

    #[test]
    fn serde_uses_string_form() {
        let ulid = Ulid::from_parts(42, 7);
        let json = serde_json::to_string(&ulid).unwrap();
        assert_eq!(json, format!("\"{}\"", ulid.encode()));
        let back: Ulid = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ulid);
    }

    #[test]
    fn seeded_generator_is_reproducible() {
        let mut a = UlidGen::seeded(2026);
        let mut b = UlidGen::seeded(2026);
        let ids_a: Vec<String> = (0..64).map(|_| a.next_id()).collect();
        let ids_b: Vec<String> = (0..64).map(|_| b.next_id()).collect();
        assert_eq!(ids_a, ids_b);
        assert_eq!(
            ids_a
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            64
        );
    }
}
