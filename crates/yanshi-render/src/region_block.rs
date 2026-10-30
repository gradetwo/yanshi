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
    /// **期 4 `lod`**：取样档位（0 = 全分辨率）。键的 `w`/`h` 是**该档位的输出像素数**
    /// （`ceil(请求宽 / 2^lod)`），不是文档像素——否则 `lod: 1` 的 50×50 与
    /// `lod: 0` 的 50×50 会共用一个键、像素互串；`lod` 维本身也进键（同尺寸不同档
    /// 也不能串）。判据：同 `(region, lod, fingerprint)` 二次导出命中。
    pub lod: u32,
    /// **★★★ 第 247 轮：**这块字节是**哪个图层范围**渲出来的 ✗ ★★★**
    ///   ＋ **∴ 为什么必须进键 ✗**（**`render_region_raw_layer` 的注释写着 ✓）**：
    ///     **∴ 只画一层 ⇒ **像素与整幅合成不同**✗
    ///       ＋ **∴ 若**与整幅共用一个键**✗ ⇒ **∴ 命中时会把**别的图层的像素**当成这一层的返回** ✓
    ///         ⇒ **∴ 那正是"**缓存键少了一个维度**"的经典 bug ✗ 而且**只在命中时发作** ✓ ★★
    ///   ＋ **∴ 取值 ✗**：**`0` ＝ **全幅合成**（**含 `active_layer` 切点那条 ✓ —— 它只改
    ///     "**重算哪些**"✗ **不改像素 ✓）｜**非 0 ＝ **某一层 id 的稳定哈希**（`layer_scope()` ✓）★
    pub scope: u64,
}

impl BlockKey {
    /// 由浮点 bbox 归一化出键 ✓（取整规则与渲染的像素栅格一致 ✓）。
    pub fn from_bbox(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self::from_bbox_lod(x, y, w, h, 0)
    }

    /// 由浮点 bbox 与 `lod` 档位归一化出键（期 4）：`w`/`h` 是该档位的输出像素数
    /// （`ceil(请求宽 / 2^lod)`）；`lod == 0` 时与 [`BlockKey::from_bbox`] 相同。
    /// `lod` 必须 ≤ 3（调用方 `export_png` 已校验）。
    pub fn from_bbox_lod(x: f64, y: f64, w: f64, h: f64, lod: u32) -> Self {
        debug_assert!(lod <= 3, "BlockKey::from_bbox_lod: lod 必须 ≤ 3");
        let step = 1u32 << lod;
        Self {
            x: x.round() as i64,
            y: y.round() as i64,
            w: (w.round().max(0.0) as u32).div_ceil(step),
            h: (h.round().max(0.0) as u32).div_ceil(step),
            // **∴ 缺省是**全幅**✗ ⇒ **∴ 既有调用点的语义**一个字都不变** ✓（**第 247 轮 ✓）★
            scope: 0,
            lod,
        }
    }

    /// **把键限定到「只渲染某一层」**✗ ★（**第 247 轮 ✓）
    ///
    /// **∴ 用法 ✗**：`BlockKey::from_bbox(..).with_scope(BlockKey::layer_scope(id))` ✓
    /// **∴ 而**全幅那条**不要调它**✗（**`scope` 保持 0 ✓）★
    #[must_use]
    pub fn with_scope(mut self, scope: u64) -> Self {
        self.scope = scope;
        self
    }

    /// **图层 id ⇒ 稳定的 scope 值 ✗**（**FNV-1a 64 ✓，与 `content_hash` 同一套常数 ✓）
    ///
    /// **∴ 为什么必须是稳定哈希 ✗**：**跨进程／跨重启**都要**同 id ⇒ 同值**✗
    ///   ⇒ **∴ 不能用 `DefaultHasher`**（**它**每次运行可能不同**✓）★
    /// **∴ 且**绝不允许返回 `0`**✗（**那是"全幅"的保留值 ✓）⇒ **∴ 撞上 0 就换成 1** ✓ ★★
    #[must_use]
    pub fn layer_scope(layer_id: &str) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in layer_id.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        if hash == 0 {
            1
        } else {
            hash
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
///
/// **★ 字节上限（RSS 修复 paint-rss，2026-10-26）★**：只按块数封顶是漏洞——
/// 全幅块 4K 是 33MB、8K 是 132MB，16 块可达 528MB。现为**块数 + 字节双上限**
/// （缺省 `DEFAULT_MAX_BYTES = 64 MiB`）：超字节时按版本最旧淘汰；单块本身超
/// 预算时仍保留这一块（与 `BufferPool` 的"至少保留一张"同理——缓存"有"比"全无"
/// 好，超预算的块只影响本块，不会把别的块挤光）。
#[derive(Debug)]
pub struct RegionBlockCache {
    blocks: BTreeMap<BlockKey, RegionBlock>,
    capacity: usize,
    /// 当前所有块 `data` 字节之和（`put`／`get` 的过期丢弃／`invalidate_before`／
    /// `clear` 都同步维护——`bytes()` 读它，不遍历）。
    bytes: usize,
    /// 字节上限（见模块说明）。
    max_bytes: usize,
    stats: RegionBlockStats,
}

/// 区域字节缓存的缺省字节上限：64 MiB（4K 全幅块 33MB 能进一块；8K 全幅块
/// 132MB 靠"单块超预算仍保留"例外进一块；两块 4K 全幅 66MB > 上限 ⇒ 旧的被汰）。
pub const DEFAULT_REGION_BLOCK_MAX_BYTES: usize = 64 * 1024 * 1024;

impl RegionBlockCache {
    /// 新建 ✓（`capacity` 是**块数**上限 ✓，至少 1 ✓；字节上限取缺省
    /// [`DEFAULT_REGION_BLOCK_MAX_BYTES`] ✓）。
    pub fn new(capacity: usize) -> Self {
        Self::new_with_byte_budget(capacity, DEFAULT_REGION_BLOCK_MAX_BYTES)
    }

    /// 新建 ✓（块数上限 + 字节上限都显式给；测试用小字节上限证明"有界" ✓）。
    pub fn new_with_byte_budget(capacity: usize, max_bytes: usize) -> Self {
        Self {
            blocks: BTreeMap::new(),
            capacity: capacity.max(1),
            bytes: 0,
            max_bytes: max_bytes.max(1),
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

    /// 当前所有块的字节之和 ✓（C2 预算闸的估算口径之一，见
    /// `Document::renderer_retained_bytes` ✓）。
    pub fn bytes(&self) -> usize {
        self.bytes
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
            if let Some(old) = self.blocks.remove(&key) {
                self.bytes = self.bytes.saturating_sub(old.data.len());
            }
            self.stats.stale += 1;
        }
        self.stats.misses += 1;
        None
    }

    /// 淘汰版本锚最旧的一块（确定性：并列时按键序 ✓，不用 HashMap 的随机序 ✗）。
    /// `except` 指定的键不参与（`put` 的字节淘汰不能把刚放进的块自己汰掉 ✓）。
    /// 返回是否真淘汰了一块。
    fn evict_oldest(&mut self, except: Option<BlockKey>) -> bool {
        let victim = self
            .blocks
            .values()
            .filter(|block| Some(block.key) != except)
            .min_by(|left, right| {
                left.version_atom
                    .cmp(&right.version_atom)
                    .then(left.key.cmp(&right.key))
            })
            .map(|block| block.key);
        match victim {
            Some(key) => {
                if let Some(old) = self.blocks.remove(&key) {
                    self.bytes = self.bytes.saturating_sub(old.data.len());
                }
                self.stats.evicted += 1;
                true
            }
            None => false,
        }
    }

    /// 放一块 ✓（超容量时按**版本最旧**淘汰 ✓；超字节上限时同样淘汰，
    /// 直到字节达标或只剩刚放进的这一块 ✓）。
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
            // 块数超限 ⇒ 淘汰版本锚最旧的一块 ✓。
            self.evict_oldest(None);
        }
        // 同键覆盖 ⇒ 先扣掉旧块的字节（否则 `bytes()` 会虚增 ✓）。
        if let Some(old) = self.blocks.get(&block.key) {
            self.bytes = self.bytes.saturating_sub(old.data.len());
        }
        self.bytes = self.bytes.saturating_add(block.data.len());
        let inserted_key = block.key;
        self.blocks.insert(block.key, block);
        // 字节超限 ⇒ 淘汰版本最旧的块直到达标；但至少保留刚放进的这一块
        // （单块超预算也保留——见模块说明；`except` 保证不汰掉自己 ✓）。
        while self.bytes > self.max_bytes && self.blocks.len() > 1 {
            if !self.evict_oldest(Some(inserted_key)) {
                break;
            }
        }
    }

    /// **丢弃所有比给定版本旧的块** ✓（提交之后调用 ✓）。
    pub fn invalidate_before(&mut self, version: u64) -> usize {
        let before = self.blocks.len();
        let mut freed = 0usize;
        self.blocks.retain(|_, block| {
            if block.version_atom >= version {
                true
            } else {
                freed = freed.saturating_add(block.data.len());
                false
            }
        });
        self.bytes = self.bytes.saturating_sub(freed);
        before - self.blocks.len()
    }

    /// 清空 ✓。
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.bytes = 0;
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
    // **★ 底色必须进指纹（`rebackground`，与上面选区/重排同型）★**：底色是每块 tile
    // 复合的基底 ⇒ 它真的改变渲染结果 ✓ ⇒ 而上面的字段一个都不含底色 ✓
    // ⇒ 于是：改底色后指纹不变 ⇒ 区域字节缓存命中那份「旧底色」的块 ✓
    // ⇒ **表现**：「改了底色画面却不动」（本工具集成测试当场抓住 ✓）。
    // **∴ 修法 ✗**：把底色按稳定序（`Value` 的 `Debug`）哈希进来 ✓ —— 与选区那条同款 ✓。
    format!("{:?}", state.background).hash(&mut h);
    let mut layers: Vec<&yanshi_core::Layer> = state.alive_layers();
    // **★ 必须覆盖**全部层** ✓ ★**（第 474 轮 **C4 判据当场抓住 ✓**）：
    // **∴ 我曾让这里**排除最上层**✗（**为了让"只改当前层"命中 ✓**）⇒ **∴ 而 `region_cache` 存的是
    // **整块渲染结果**（**含所有层 ✓**）⇒ **∴ 排除上层 ⇒ 改上层后**返回旧块 ✗ ⇒ **＝ 撒谎 ✓**
    //（**实测**：`h3 == h4` ✗ —— **改最上层后 `raw_url` 哈希没变 ✓**）。
    // **∴ 正解** ✓：**块指纹**必须覆盖全部层 ✓**；**要"只重画当前层" ⇒ **应当把 **below 缓存
    // 接进块渲染 ✓**（**分段复用 ✓**），**而**不是**让块指纹忽略上层 ✗**。
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
            // 蒙版位图化后 `Debug` 字符串会爆炸 ⇒ 用结构化指纹。
            mask.hash_fingerprint(&mut h);
        }
        // **★ `z_index` 必须进指纹（Y-08 期 4，R10）★**：重排（`ReorderObjects`）
        //   与 `set_property z_index` 只改 `z_index`、不走 supersede
        //   ⇒ `current_version` 不变 ⇒ 不含 z 的指纹在重排后不变
        //   ⇒ 区域字节缓存命中旧块 ⇒ "排了但画面不动"。
        //   （`set_property` 单改因 supersede 推 `current_version` 而
        //   间接失效；只有重排这条路会漏，故此处只补 `z_index`。）
        let mut objs: Vec<String> = state
            .objects
            .values()
            .filter(|o| o.layer_id == l.id && o.deleted_by.is_none())
            // **★ 对象变换必须进指纹**：几何原子（flip/rotate/scale）只复合
            // `transform`、不 supersede 对象 ⇒ 版本与 data 都不变；不含变换时
            // 区域字节缓存会命中变换前的旧块、画面不动（与选区/底色同类坑）。
            .map(|o| {
                format!(
                    "{}:{}:{:?}:{}:{:?}",
                    o.id, o.z_index, o.current_version, o.data, o.transform.matrix
                )
            })
            .collect();
        objs.sort();
        objs.hash(&mut h);
    }
    // **★ 选区必须进指纹 ✗ ★**（第 934 轮 ✓；**修真缺陷 ✓**）：
    //   **∴ 为什么 ✗**：**选区**在**渲染期**裁剪落笔 ✓（**设计 4.4 路线 A ✓**）
    //     ⇒ **∴ 它**真的**改变渲染结果** ✗** ⇒ **∴ 而**上面的字段**一个都不含选区 ✓**
    //       ⇒ **∴ 于是**：**删掉选区后**指纹**不变 ✗**
    //         ⇒ **∴ 区域字节缓存**命中**那份"**裁剪过的**"旧块 ✓**
    //           ⇒ **★ 表现**："**选区外仍无改动 ⇒ **裁剪被错误地烘焙进了像素** ✓" ★**** ✓✓
    //   **∴ 实测（**四条判据 ✓）**：**`yanshi-server/tests/selection_clip.rs` 的
    //     `a_selection_clips_new_painting_and_is_reversible` ＋ `_a_fill_` ＋ `_text` ＋ `_liquify`
    //     **全红 ✗**，**报**"**删除选区后…选区外仍无改动 ✓"**；
    //     **∴ 而** `_retouch` 与 `without_a_selection_…` **通过 ✓**
    //       ⇒ **∴ 因为**"**建选区后**那次是**首次渲染 ⇒ **∴ 无缓存 ⇒ **∴ 对 ✓**** ✓✓
    //   **∴ 修法 ✗**：**把选区集合按**稳定序**哈希进来 ✓** —— **∴ 用 `format!("{s:?}")` ✓
    //     ⇒ **∴ 它**含 `deleted_by` ✗**（**∴ 而**删选区**正是**改它 ✓）**；
    //     **∴ 排序 ⇒ **∴ 与 map 的遍历顺序无关 ✓**** ✓✓
    //   **∴ 两面（**如实 ✓）**：**收益**＝**选区一变**缓存**立即失效 ⇒ **∴ 删选区后**正确重算 ✓**
    //     （**∴ 且**这**正是**目标第 6 条「**绝不许拿旧图冒充**」 ✓）**；
    //     **∴ 代价**＝**选区一变**就**丢掉区域缓存**（**重算一次 ✓）——
    //       **∴ 而**那是**必需的** ✗**（**∴ 因为**选区**真的**改变渲染结果 ✓）**
    //       ⇒ **∴ 不是**性能退化 ✗，**而是**正确性 ✓**** ✓✓
    let mut selections: Vec<String> = state
        .selections
        .values()
        .map(|selection| format!("{selection:?}"))
        .collect();
    selections.sort();
    selections.hash(&mut h);
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

    /// **只改 `z_index`，指纹必须变（Y-08 期 4，R10）**：重排只改 z，
    /// 不走 supersede ⇒ `current_version` 不变 ⇒ 旧指纹会把重排后的
    /// 渲染当成缓存命中，发旧像素（"排了但画面不动"）。
    #[test]
    fn fingerprint_changes_when_only_z_index_changes() {
        use serde_json::json;
        use yanshi_core::fold::fold_atoms;
        use yanshi_core::state::DocumentState;
        use yanshi_core::{Atom, AtomKind};

        fn atom(kind: AtomKind, id: &str, payload: serde_json::Value) -> Atom {
            Atom::new(kind, "human:1", "session:a", payload).with_id(id)
        }
        let mut atoms = Vec::new();
        let push = |atoms: &mut Vec<Atom>, atom: Atom| {
            let mut atom = atom;
            atom.seq = atoms.len() as u64 + 1;
            atoms.push(atom);
        };
        push(
            &mut atoms,
            atom(
                AtomKind::CreateDocument,
                "a_doc",
                json!({"doc_id": "doc_1", "width": 16, "height": 16}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::CreateLayer,
                "a_layer",
                json!({"layer_id": "layer_1"}),
            ),
        );
        for (id, object_id) in [("a_o1", "obj_1"), ("a_o2", "obj_2")] {
            push(
                &mut atoms,
                atom(
                    AtomKind::CreateObject,
                    id,
                    json!({"object_id": object_id, "layer_id": "layer_1", "type": "stroke"}),
                ),
            );
        }
        let before = fold_atoms(DocumentState::empty(), &atoms).state;
        let fp_before = region_fingerprint(&before);
        push(
            &mut atoms,
            atom(
                AtomKind::ReorderObjects,
                "a_reorder",
                json!({"layer_id": "layer_1", "order": ["obj_2", "obj_1"]}),
            ),
        );
        let after = fold_atoms(DocumentState::empty(), &atoms).state;
        assert_eq!(
            after.objects_in_layer("layer_1")[0].id,
            "obj_2",
            "前置：重排须真的改顺序"
        );
        assert_ne!(
            region_fingerprint(&after),
            fp_before,
            "只改 z_index 也必须让指纹变（否则缓存命中旧块）"
        );
    }

    /// **只改底色，指纹必须变（`rebackground`）**：底色是复合基底 ⇒ 改它不改
    /// 指纹 ⇒ 区域字节缓存命中旧底色的块 ⇒ 「改了底色画面却不动」。
    /// （本工具集成测试当场抓住：改底色后中心像素仍是白 ✗。）
    #[test]
    fn fingerprint_changes_when_only_background_changes() {
        use serde_json::json;
        use yanshi_core::fold::fold_atoms;
        use yanshi_core::state::DocumentState;
        use yanshi_core::{Atom, AtomKind};

        fn atom(kind: AtomKind, id: &str, payload: serde_json::Value) -> Atom {
            Atom::new(kind, "human:1", "session:a", payload).with_id(id)
        }
        let mut atoms = Vec::new();
        let push = |atoms: &mut Vec<Atom>, atom: Atom| {
            let mut atom = atom;
            atom.seq = atoms.len() as u64 + 1;
            atoms.push(atom);
        };
        push(
            &mut atoms,
            atom(
                AtomKind::CreateDocument,
                "a_doc",
                json!({"doc_id": "doc_1", "width": 16, "height": 16,
                    "background": {"r": 255, "g": 255, "b": 255, "a": 255}}),
            ),
        );
        let before = fold_atoms(DocumentState::empty(), &atoms).state;
        let fp_before = region_fingerprint(&before);
        push(
            &mut atoms,
            atom(
                AtomKind::SetBackground,
                "a_bg",
                json!({"background": {"r": 30, "g": 60, "b": 120, "a": 255}}),
            ),
        );
        let after = fold_atoms(DocumentState::empty(), &atoms).state;
        assert_eq!(
            after.background,
            json!({"r": 30, "g": 60, "b": 120, "a": 255}),
            "前置：底色原子须真的改底色"
        );
        assert_ne!(
            region_fingerprint(&after),
            fp_before,
            "只改底色也必须让指纹变（否则缓存命中旧底色的块）"
        );
    }

    /// **RSS 修复 paint-rss：字节上限淘汰最旧的块，且 `bytes()` 与内容一致**。
    ///
    /// 变异：去掉 `put` 末尾的字节淘汰循环 ⇒ 第三块放进后 `bytes()` 仍是 3 MiB ⇒ 红。
    #[test]
    fn byte_budget_evicts_the_oldest_block() {
        // 预算 2 MiB；每块 1 MiB（512×512×4 B）。
        let mut cache = RegionBlockCache::new_with_byte_budget(16, 2 * 1024 * 1024);
        let a = BlockKey::from_bbox(0.0, 0.0, 512.0, 512.0);
        let b = BlockKey::from_bbox(512.0, 0.0, 512.0, 512.0);
        let c = BlockKey::from_bbox(1024.0, 0.0, 512.0, 512.0);
        cache.put(block(a, 1, 10));
        cache.put(block(b, 5, 20));
        assert_eq!(cache.bytes(), 2 * 1024 * 1024, "两块 1 MiB 应正好占满预算");
        cache.put(block(c, 9, 30));
        assert_eq!(cache.len(), 2, "超字节后只能留两块");
        assert_eq!(cache.bytes(), 2 * 1024 * 1024, "字节数必须回到预算内");
        assert!(cache.get(a, 1).is_none(), "版本最旧的 a 应被淘汰");
        assert!(cache.get(b, 5).is_some(), "较新的 b 应还在");
        assert!(cache.get(c, 9).is_some(), "刚放进的 c 应还在");
        assert_eq!(cache.stats().evicted, 1);
    }

    /// **RSS 修复 paint-rss：单块本身超预算也保留**（不把别的块挤光、不丢刚放的块）。
    #[test]
    fn single_oversize_block_is_kept() {
        let mut cache = RegionBlockCache::new_with_byte_budget(16, 2 * 1024 * 1024);
        let small = BlockKey::from_bbox(0.0, 0.0, 512.0, 512.0);
        let big = BlockKey::from_bbox(0.0, 0.0, 2048.0, 2048.0); // 16 MiB
        cache.put(block(small, 1, 10));
        cache.put(block(big, 2, 20));
        assert_eq!(cache.len(), 1, "大块进来后小块应被汰掉、只剩大块");
        assert!(cache.get(big, 2).is_some(), "超预算的单块本身必须保留");
        assert_eq!(cache.bytes(), 16 * 1024 * 1024);
    }

    /// **RSS 修复 paint-rss：同键覆盖与过期丢弃都要维护 `bytes()`**（恒等式：
    /// `bytes()` 恒等于各块 `data.len()` 之和，判据不靠"数一遍"而靠记账一致）。
    #[test]
    fn bytes_accounting_survives_overwrite_and_stale_drop() {
        let mut cache = RegionBlockCache::new(16);
        let key = BlockKey::from_bbox(0.0, 0.0, 512.0, 512.0);
        cache.put(block(key, 7, 200));
        assert_eq!(cache.bytes(), 1024 * 1024);
        // 同键同尺寸覆盖：字节数不变。
        cache.put(block(key, 7, 201));
        assert_eq!(cache.bytes(), 1024 * 1024, "同键覆盖不得虚增字节");
        // 版本一变再取：旧块被当过期丢弃，字节归零。
        assert!(cache.get(key, 8).is_none());
        assert_eq!(cache.bytes(), 0, "过期丢弃后字节应归零");
        assert_eq!(cache.len(), 0);
        cache.clear();
        assert_eq!(cache.bytes(), 0);
    }
}
