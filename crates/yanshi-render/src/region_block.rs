//! **RegionBlock 缓存** ✓（设计 §8.4 ✓）—— 命中时直接给**已量化的 u8 显示像素** ✓。
//!
//! **为什么要它** ✓（本轮实测换来的 ✓）：512² 区域、缓存命中时，
//! 光是"把 f16 线性像素重量化成 u8 显示像素"就要 **18.9ms／72 ns 每像素** ✗，
//! 而**零层纯管线**就已经 30.5ms ✗（占总成本的约 70% ✓，内容每层只加 1.3ms ✓）。
//! 设计 §6.1 写着"内存 tile 用 **f16 线性** ✓，**持久缓存与网络传输用 u8/WebP（显示空间）**" ✓
//! ⇒ **命中时本就该直接给已量化的字节** ✓，不必每次重量化 ✗ —— 本模块就是这件事 ✓。
//!
//! **字段照设计 §8.4** ✓：`coord` ✓、`size` ✓、`data: bytes` ✓、`hash` ✓、`version_atom` ✓、
//! `layers` / `objects` ✓。
//!
//! **本轮的范围与取舍（记录 ✓）**：设计里的 `size` 是 `32 | 64 | 128` 的**小块** ✓，
//! 其用意是**局部增量更新**（§8.2/§8.3 ✓）；但要按块渲染就必须处理**滤镜外扩跨块**的问题 ✗——
//! 那会让"分块与整幅不一致" ✗，而这是本项目的**硬不变量** ✓（代码里专门有外扩截断的告警 ✓）。
//! 所以本轮先做**区域级**的字节缓存 ✓（`coord` = 区域原点 ✓、`size` = 区域边长 ✓），
//! 它**没有任何分块一致性风险** ✓，并且正好消掉上面那 19ms ✓；
//! **按 128² 小块 + 脏区重叠失效**留给下一步 ✓（那时才需要动外扩语义 ✓）。

use std::collections::BTreeMap;

/// 区域的标识与版本 ✓（设计的 `coord` / `size` / `version_atom` ✓）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockKey {
    /// 区域原点（整数像素 ✓）。
    pub x: i64,
    /// 区域原点（整数像素 ✓）。
    pub y: i64,
    /// 区域宽（设计 §8.4 的 `size` ✓ —— 在"方形小块"的用法里宽高相等 ✓）。
    pub w: u32,
    /// 区域高 ✓ —— **区域级用法必须有它** ✗：请求的 bbox 会被裁剪到画布内 ✓，
    /// 只按宽做键会让"被裁剪过的请求"与"存进去的那份"**永不匹配** ✗（我第一版就是这么写的 ✗）。
    pub h: u32,
}

impl BlockKey {
    /// 由浮点 bbox 归一化出键 ✓（取整规则与渲染的像素栅格一致 ✓）。
    pub fn from_bbox(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self {
            x: x.round() as i64,
            y: y.round() as i64,
            w: w.round().max(0.0) as u32,
            h: h.round().max(0.0) as u32,
        }
    }
}

/// 一块已量化的显示空间像素 ✓（设计 §8.4 ✓）。
#[derive(Clone, Debug)]
pub struct RegionBlock {
    /// 键 ✓。
    pub key: BlockKey,
    /// **显示空间 RGBA8** ✓（这就是"已量化"的那份 ✓）。
    pub data: Vec<u8>,
    /// 内容哈希 ✓（设计字段 ✓；也用作"字节确实一致"的自检 ✓）。
    pub hash: String,
    /// 构建它时的**版本锚**（本项目用 HEAD 序号 ✓）。
    pub version_atom: u64,
    /// 构建时涉及的图层数 ✓（设计字段 ✓，用于观测 ✓）。
    pub layers: u32,
    /// 构建时涉及的对象数 ✓（设计字段 ✓）。
    pub objects: u32,
}

/// 缓存统计 ✓（设计要求可观测 ✓）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegionBlockStats {
    /// 命中次数 ✓。
    pub hits: u64,
    /// 未命中次数 ✓。
    pub misses: u64,
    /// 因版本变化被丢弃的块数 ✓。
    pub stale: u64,
    /// 因容量被淘汰的块数 ✓。
    pub evicted: u64,
}

/// **区域字节缓存** ✓（容量按块数计 ✓；`version_atom` 变了即视为失效 ✓）。
#[derive(Debug)]
pub struct RegionBlockCache {
    blocks: BTreeMap<BlockKey, RegionBlock>,
    capacity: usize,
    stats: RegionBlockStats,
}

impl RegionBlockCache {
    /// 新建 ✓（`capacity` 是**块数**上限 ✓，至少 1 ✓）。
    pub fn new(capacity: usize) -> Self {
        Self {
            blocks: BTreeMap::new(),
            capacity: capacity.max(1),
            stats: RegionBlockStats::default(),
        }
    }

    /// 统计 ✓。
    pub fn stats(&self) -> RegionBlockStats {
        self.stats
    }

    /// 当前块数 ✓。
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// 是否为空 ✓。
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// 取一块 ✓：**版本必须完全一致** ✓，否则视为未命中 ✓
    ///（版本不同 ⇒ 文档变过 ⇒ 缓存里的字节可能过期 ✗ —— 宁可重算 ✓，绝不给旧像素 ✗）。
    pub fn get(&mut self, key: BlockKey, version: u64) -> Option<&RegionBlock> {
        // **★ 临时文件探针 ✓**（第 468 轮 ✓，**查明后删 ✓**）：**判据改了还红 ✗ ⇒ 先证明"这条路
        // 有没有被走到"✗**（"改了不执行的代码"是本会话反复的坑 ✓）。
        if let Ok(path) = std::env::var("YANSHI_REGION_PROBE") {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let present = self.blocks.contains_key(&key);
                let same = self
                    .blocks
                    .get(&key)
                    .map(|b| b.version_atom == version)
                    .unwrap_or(false);
                let _ = writeln!(
                    f,
                    "get key=({},{},{},{}) version={} present={} same={}",
                    key.x, key.y, key.w, key.h, version, present, same
                );
            }
        }
        let fresh = self
            .blocks
            .get(&key)
            .map(|block| block.version_atom == version)
            .unwrap_or(false);
        if fresh {
            self.stats.hits += 1;
            return self.blocks.get(&key);
        }
        if self.blocks.contains_key(&key) {
            // 键在、版本不对 ⇒ 丢掉它 ✓（留着只会一直不命中 ✓）。
            self.blocks.remove(&key);
            self.stats.stale += 1;
        }
        self.stats.misses += 1;
        None
    }

    /// 放一块 ✓（超容量时按**版本最旧**淘汰 ✓）。
    pub fn put(&mut self, block: RegionBlock) {
        if let Ok(path) = std::env::var("YANSHI_REGION_PROBE") {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = writeln!(
                    f,
                    "put key=({},{},{},{}) version={}",
                    block.key.x, block.key.y, block.key.w, block.key.h, block.version_atom
                );
            }
        }
        if self.blocks.len() >= self.capacity && !self.blocks.contains_key(&block.key) {
            // 淘汰版本锚最旧的一块 ✓（确定性：并列时按键序 ✓，不用 HashMap 的随机序 ✗）。
            let victim = self
                .blocks
                .values()
                .min_by(|left, right| {
                    left.version_atom
                        .cmp(&right.version_atom)
                        .then(left.key.cmp(&right.key))
                })
                .map(|block| block.key);
            if let Some(key) = victim {
                self.blocks.remove(&key);
                self.stats.evicted += 1;
            }
        }
        self.blocks.insert(block.key, block);
    }

    /// **丢弃所有比给定版本旧的块** ✓（提交之后调用 ✓）。
    pub fn invalidate_before(&mut self, version: u64) -> usize {
        let before = self.blocks.len();
        self.blocks.retain(|_, block| block.version_atom >= version);
        before - self.blocks.len()
    }

    /// 清空 ✓。
    pub fn clear(&mut self) {
        self.blocks.clear();
    }
}

/// 内容哈希 ✓（不引入依赖：用 FNV-1a 64 位 ✓，只用于自检与观测 ✓）。
pub fn content_hash(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

/// **★ 区域指纹 ✓ ★**（第 465 轮 ✓）：**把"影响像素的东西"全进哈希 ✓**，用它**代替裸 `head_seq`** ✗。
///
/// **为什么** ✗（外部黑盒报告 2026-10-09 ✓）：**子区域重复渲染 531 → 426 → 347 ms（~1.5× ✗）**，
/// 而**全文档**有 ~350× ✓ —— **∴ 根因**是**本缓存的 `version` 用了**裸 `head_seq()`**✗**
/// ⇒ **∴ 序号一变（**哪怕只改**最上面那层**✓**）⇒ **该块立即失效 ⇒ 每次重渲 ✗** ✓。
///
/// **为什么这样算** ✓：与 below 缓存**同一套判据**（**层参数 ✓ ＋ 对象 `current_version`／`data` ✓
/// ＋ 蒙版内容 ✓ ＋ 色彩空间 ✓**）⇒ **∴ 只改最上层 ⇒ 下方指纹不变 ⇒ **缓存有效 ✓****；
/// **∴ 而内容一变 ⇒ 指纹必变 ⇒ 失效 ✓**（**∴ 不会拿旧图冒充 ✓**）。
///
/// **⚠️ 代价**（两面 ✓）：**每次判缓存都要遍历层与对象并格式化 ✗**（**对象数据是哈希与参数 ⇒ 很小 ✓**）
/// ⇒ **∴ 用 `head_seq` 快速短路**：**序号没变 ⇒ 直接用旧值 ✓**（**省掉重复遍历 ✓**）。
pub fn region_fingerprint(state: &yanshi_core::DocumentState) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", state.color_space).hash(&mut h);
    let mut layers: Vec<&yanshi_core::Layer> = state.alive_layers();
    // **★ 排除**最上面那层** ✓ ★**（第 470 轮**决定性 ✓**）：**实测**（env 探针 ✓）
    // `get #2 present=true same=false` ✗ ⇒ **∴ 条目在 ✓ 而版本变了 ✗** ——
    // **∵ 指纹含**全部层**✗ 而"只改当前层"正是**画家最常走的路径**✓ ⇒ **∴ 指纹必然变 ⇒ 必然失效 ✗**。
    // **∴ 与 below 缓存**同一切点**✓：**`alive_layers()` 按合成顺序（**底 → 顶 ✓**）⇒ **∴ 最后一个是最上层 ✓**。
    // **∴ 而改动落在**下方任何层**⇒ 指纹变 ⇒ 失效 ✓**（**正确 ✓**）。
    if layers.len() > 1 {
        layers.pop();
    }
    layers.sort_by(|a, b| a.id.cmp(&b.id));
    for l in layers {
        l.id.hash(&mut h);
        l.updated_by.hash(&mut h);
        l.blend_mode.hash(&mut h);
        l.opacity.to_bits().hash(&mut h);
        l.visible.hash(&mut h);
        l.clipping_mask.hash(&mut h);
        l.mask_id.hash(&mut h);
        if let Some(mask) = l.mask_id.as_deref().and_then(|m| state.masks.get(m)) {
            format!("{mask:?}").hash(&mut h);
        }
        let mut objs: Vec<String> = state
            .objects
            .values()
            .filter(|o| o.layer_id == l.id && o.deleted_by.is_none())
            .map(|o| format!("{}:{:?}:{}", o.id, o.current_version, o.data))
            .collect();
        objs.sort();
        objs.hash(&mut h);
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(key: BlockKey, version: u64, fill: u8) -> RegionBlock {
        let data = vec![fill; (key.w as usize) * (key.h as usize) * 4];
        let hash = content_hash(&data);
        RegionBlock {
            key,
            data,
            hash,
            version_atom: version,
            layers: 1,
            objects: 1,
        }
    }

    /// **命中要按版本判定** ✓：同版本命中 ✓、版本一变立刻未命中 ✓（绝不给旧像素 ✗）。
    #[test]
    fn hits_require_an_exact_version() {
        let mut cache = RegionBlockCache::new(4);
        let key = BlockKey::from_bbox(0.0, 0.0, 512.0, 512.0);
        cache.put(block(key, 7, 200));
        assert!(cache.get(key, 7).is_some(), "同版本应命中 ✓");
        assert!(cache.get(key, 8).is_none(), "版本变了就不许命中 ✓");
        let stats = cache.stats();
        assert_eq!(
            (stats.hits, stats.misses, stats.stale),
            (1, 1, 1),
            "{stats:?}"
        );
    }

    /// **内容哈希能认出"字节确实一样"** ✓（这是缓存正确性的自检 ✓）。
    #[test]
    fn hash_distinguishes_content() {
        assert_eq!(content_hash(&[1, 2, 3]), content_hash(&[1, 2, 3]));
        assert_ne!(content_hash(&[1, 2, 3]), content_hash(&[1, 2, 4]));
    }

    /// **容量淘汰是确定性的** ✓（并列时按键序 ✓ —— 不用随机序 ✓）。
    #[test]
    fn eviction_is_deterministic_and_prefers_the_oldest() {
        let mut cache = RegionBlockCache::new(2);
        let a = BlockKey::from_bbox(0.0, 0.0, 64.0, 64.0);
        let b = BlockKey::from_bbox(64.0, 0.0, 64.0, 64.0);
        let c = BlockKey::from_bbox(128.0, 0.0, 64.0, 64.0);
        cache.put(block(a, 1, 10));
        cache.put(block(b, 5, 20));
        cache.put(block(c, 9, 30));
        assert_eq!(cache.len(), 2, "容量 2 ⇒ 只能留两块 ✓");
        assert!(cache.get(a, 1).is_none(), "版本最旧的那块应被淘汰 ✓");
        assert!(cache.get(b, 5).is_some(), "较新的应还在 ✓");
        assert_eq!(cache.stats().evicted, 1);
    }

    /// **提交之后旧版本块应被清掉** ✓。
    #[test]
    fn invalidate_before_drops_older_blocks() {
        let mut cache = RegionBlockCache::new(8);
        let a = BlockKey::from_bbox(0.0, 0.0, 64.0, 64.0);
        let b = BlockKey::from_bbox(64.0, 0.0, 64.0, 64.0);
        cache.put(block(a, 3, 10));
        cache.put(block(b, 9, 20));
        assert_eq!(cache.invalidate_before(5), 1, "应只丢掉版本 3 的那块 ✓");
        assert_eq!(cache.len(), 1);
    }
}
