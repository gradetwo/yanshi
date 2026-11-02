//! **对象前置工作缓存**（#1 第一期，路线 A）：把渲染循环里每对象必做的
//! 前置工作（解析＋变换＋包围盒）按对象缓存一次，二次渲染直接复用。
//!
//! **为什么是这里**：单文档对象数 N 上升时，`render_layer_objects` 的前置工作
//! 逐对象线性增长（每对象两次解析：一次取图元、一次在 `object_bbox_in` 里取
//! 包围盒），是总耗时随 N² 增长的两处前置成本之一。
//!
//! **键的安全性**：键＝（对象 id、current_version、变换矩阵＋枢轴位表示、
//! `data` 结构哈希、对象类型标记）——即提案原键（版本＋data 哈希＋变换）
//! 再加两项补全（类型不在 `data` 里、枢轴不在矩阵里，缺一则撞键）。
//! `fold` 的实证（见 `crates/yanshi-core/src/fold.rs`）说明版本与 data
//! 在真实原子流里是同变的：`data` 的一切改动都发生在 `supersede_object`
//! 之后（`SetProperty` 先 `supersede` 再 `apply_property_to_object`，快照
//! 写／删也在同一原子内；`Fill`/`Erase`/`Retouch`/`Liquify`/`Supersede`／
//! `Move`/`Transform` 也都先 `supersede`），而 `supersede` 必推
//! `current_version` 为新原子 id。但**键不能只靠版本**：手写状态
//! （测试 helper 固定写 `current_version = "a1"`）会把不同内容写进同一
//! （id、版本）——实测：`heal` 测试在同一渲染器上先后插入同 id、同版本、
//! 不同 `data` 的对象，省掉 data 哈希的键撞了、heal 画成 clone 的旧图元。
//! 故 data 哈希必须在内（每对象一次无分配的结构遍历，远廉于解析）；
//! 版本仍保留（与 `fp_cache` 同形，且真实原子流里它是内容变化的主信号）。
//! * 不推版本的改动只有两类：`transform`（`Move`/`Transform` 的组变换、
//!   `SetProperty` 内的组成员平移都直接写矩阵）与 `z_index`（只影响顺序，
//!   不影响单对象预处理）⇒ **变换矩阵＋枢轴的位表示必须单列入键**，
//!   `z_index` 不入键。
//! * 实例：解析链上的每一环（`master_ref` 指向的对象）都入键；链上任一环
//!   的版本／变换／data 一变 ⇒ 键变 ⇒ 旧条目不用。快照实例（`sync_policy: none`）
//!   的解析不走链（`resolve_instance` 先于链行走返回快照），故只跟自身。
//!   链断裂（master 缺失／已删／成环／超 64 层）⇒ 不建键、不缓存，
//!   走今日原路径（`resolve_instance` 返回 `None` ⇒ `Unsupported`，不画）。
//!   因此**已删 master 的旧条目永远查不到**：删 master 后建键直接失败，
//!   恢复后键与旧条目相同、内容也相同，命中正确。
//!
//! **条目**：变换后图元＋包围盒（包围盒由 `object_bbox_in` 原样算出，与循环
//! 今日的裁剪口径同一函数、同一输入）。整层作用图元（`Adjustment`／`Filter`／
//! `Unsupported`）循环本来就不算包围盒，条目里记 `None`。
//!
//! **容量与开关**：上限 262144 条，超限整表清空（正确性不受影响，
//! 只多算一次）；渲染器重建时随之丢弃。
//! 上限为什么是 262144 而不是沿用 `fp_cache` 的 65536：渲染每遍都顺序
//! 扫描全部对象，低于工作集的任何淘汰（整表清空、LRU 皆然）都会让下一
//! 遍命中为零——实测：100k 文档在 65536 上限下冷暖两遍命中皆 0。
//! 262144 覆盖提案的 10 万目标（留 2.6 倍余量）；超此规模的文档仍会
//! 撞悬崖（每遍清表、暖渲无收益），届时须换按字节预算的淘汰（后续工作，
//! 不在本期）。
//! `YANSHI_PREP_CACHE=0` 关掉缓存走原路径（A/B 对比与判据用；开关读一次
//! 存在渲染器字段里，不逐对象读环境变量）。

use std::collections::HashMap;

use serde_json::Value;
use yanshi_core::{Bbox, DocumentState, Object, ObjectType};

use crate::object::{
    object_bbox_in, parse_object, resolve_instance, transform_primitive, Primitive,
};

/// 前置缓存容量上限（条目数；与 `fp_cache` 同例，超限整表清空）。
pub const PREP_CACHE_CAP: usize = 262144;

/// 解析链上的一环：对象身份＋版本＋变换（位表示，`-0.0` 与 `0.0` 区分、
/// `NaN` 负载不坍缩——位表示比 `f64` 本身更严，不会把两个不同的变换当成一个）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrepChainLink {
    /// 对象 id。
    pub id: String,
    /// `current_version`（`None` ⇒ 无版本对象，不缓存：与 `fp_cache` 同例，
    /// 无版本对象每次重算，不进表）。
    pub version: Option<String>,
    /// `transform.matrix` 的位表示。
    pub matrix: [u64; 6],
    /// `transform.pivot` 的位表示（旧指纹与旧 `fp_cache` 键都漏了它，
    /// 而 `transform_primitive` 会把枢轴折进矩阵 ⇒ 枢轴变化必须换键）。
    pub pivot: [u64; 2],
    /// `data` 的结构哈希（无分配遍历，见 [`hash_data_value`]；64 位碰撞
    /// 概率与本仓既有的 `region_fingerprint` 同级，可接受）。
    pub data_hash: u64,
    /// 对象类型标记（`data` 里没有类型，`parse_object` 按类型分派 ⇒
    /// 同 id、同版本、同 data、不同类型也必须换键）。
    pub object_type: u8,
}

/// 前置缓存的键。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PrepCacheKey {
    /// 普通对象：只跟自身。
    Plain(PrepChainLink),
    /// 快照实例（`sync_policy: none`）：解析不走链，只跟自身。
    InstanceSnapshot(PrepChainLink),
    /// 跟随实例：自身＋解析链（链上每一环都是 `master_ref` 指向的对象，
    /// 按行走顺序排列；最终 master 在链尾）。
    InstanceChain {
        /// 实例自身。
        head: PrepChainLink,
        /// 解析链（不含自身）。
        chain: Vec<PrepChainLink>,
    },
}

/// 单条缓存条目：变换后图元＋包围盒（整层作用图元记 `None`）。
#[derive(Debug, Clone)]
pub struct PrepEntry {
    /// 变换后图元（循环 `match primitive` 直接消费，须为 owned 克隆）。
    pub primitive: Primitive,
    /// 包围盒（文档坐标；`None` ⇒ 整层作用图元，循环不做几何裁剪）。
    pub bbox: Option<Bbox>,
}

/// 前置缓存的可观测读数（判据据此断言"二次渲染命中"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrepCacheStats {
    /// 命中次数。
    pub hits: u64,
    /// 未命中次数（键已建成但表里没有 ⇒ 现算一次再存入）。
    pub misses: u64,
    /// 当前条目数。
    pub entries: usize,
}

/// 前置缓存本体（渲染器以 `Mutex` 持有，与 `below`/`above` 同例）。
#[derive(Debug, Default)]
pub struct PrepCache {
    map: HashMap<PrepCacheKey, PrepEntry>,
    hits: u64,
    misses: u64,
}

impl PrepCache {
    /// 空缓存。
    pub fn new() -> Self {
        Self::default()
    }

    /// 查：命中 ⇒ 计数并返回克隆；未命中 ⇒ 计数并返回 `None`
    /// （调用方现算后用 [`Self::insert`] 存入）。
    pub fn lookup(&mut self, key: &PrepCacheKey) -> Option<PrepEntry> {
        match self.map.get(key) {
            Some(entry) => {
                self.hits += 1;
                Some(entry.clone())
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    /// 存：超限先整表清空再存（正确性不受影响，只多算一次；
    /// 见模块文档：顺序全扫描下容量必须覆盖工作集才有暖收益）。
    pub fn insert(&mut self, key: PrepCacheKey, entry: PrepEntry) {
        if self.map.len() >= PREP_CACHE_CAP {
            self.map.clear();
        }
        self.map.insert(key, entry);
    }

    /// 读数。
    pub fn stats(&self) -> PrepCacheStats {
        PrepCacheStats {
            hits: self.hits,
            misses: self.misses,
            entries: self.map.len(),
        }
    }
}

/// 环境开关的缺省值：`YANSHI_PREP_CACHE=0` ⇒ 关，其余（含未设）⇒ 开。
/// 渲染器构造时读一次（见 `Renderer::new`），不逐对象读环境变量。
pub fn prep_cache_enabled_from_env() -> bool {
    !std::env::var_os("YANSHI_PREP_CACHE").is_some_and(|v| v == "0")
}

/// `data` 的结构哈希：无分配递归遍历（不经过 `Debug` 格式化字符串，
/// 故远廉于旧 `region_fingerprint` 的每对象格式化）。
/// 数字按类型分流（整数与浮点 `1` 与 `1.0` 哈希不同 ⇒ 只丢共享、不错判）；
/// 对象按 `serde_json::Map` 的迭代序（缺省 `BTreeMap` ⇒ 有序、确定）。
fn hash_data_value(value: &Value) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    fn walk<H: Hasher>(value: &Value, state: &mut H) {
        match value {
            Value::Null => 0u8.hash(state),
            Value::Bool(b) => {
                1u8.hash(state);
                b.hash(state);
            }
            Value::Number(n) => {
                2u8.hash(state);
                if let Some(u) = n.as_u64() {
                    0u8.hash(state);
                    u.hash(state);
                } else if let Some(i) = n.as_i64() {
                    1u8.hash(state);
                    i.hash(state);
                } else if let Some(f) = n.as_f64() {
                    2u8.hash(state);
                    f.to_bits().hash(state);
                }
            }
            Value::String(s) => {
                3u8.hash(state);
                s.hash(state);
            }
            Value::Array(items) => {
                4u8.hash(state);
                items.len().hash(state);
                for item in items {
                    walk(item, state);
                }
            }
            Value::Object(map) => {
                5u8.hash(state);
                map.len().hash(state);
                for (key, item) in map {
                    key.hash(state);
                    walk(item, state);
                }
            }
        }
    }
    let mut h = DefaultHasher::new();
    walk(value, &mut h);
    h.finish()
}

/// 对象类型标记：显式匹配（不用 `as u8`，变体重排也不改变标记值）。
fn object_type_tag(object_type: ObjectType) -> u8 {
    match object_type {
        ObjectType::Stroke => 0,
        ObjectType::Shape => 1,
        ObjectType::Text => 2,
        ObjectType::Adjustment => 3,
        ObjectType::Filter => 4,
        ObjectType::RasterPatch => 5,
        ObjectType::Retouch => 6,
        ObjectType::Liquify => 7,
        ObjectType::Instance => 8,
        ObjectType::Group => 9,
        ObjectType::Path => 10,
    }
}

fn link_of(object: &Object) -> PrepChainLink {
    PrepChainLink {
        id: object.id.clone(),
        version: object.current_version.clone(),
        matrix: object.transform.matrix.map(f64::to_bits),
        pivot: object.transform.pivot.map(f64::to_bits),
        data_hash: hash_data_value(&object.data),
        object_type: object_type_tag(object.object_type),
    }
}

/// 为单个对象建前置缓存键（`None` ⇒ 不缓存，走原路径现算）。
///
/// 与 [`resolve_instance`] 逐支镜像：快照路径先于链行走返回，链行走
/// 每次取 `master_ref.object_id`、查 `state.objects`、判已删、遇实例继续、
/// 遇最终 master 停止，上限 64 层。任一支失败（无 `master_ref`、master
/// 缺失／已删、超深）⇒ `None`（今日原路径同样画不出东西，不缓存无害）。
pub fn prep_key_for(state: &DocumentState, object: &Object) -> Option<PrepCacheKey> {
    // 无版本对象不缓存（与 `fp_cache` 同例）：版本是 data 的唯一覆盖信号，
    // 没有它就不能断言"内容没变"（`?` 在此即"无版本 ⇒ 整键为 None"）。
    let _ = object.current_version.as_ref()?;
    let head = link_of(object);
    if object.object_type != ObjectType::Instance {
        return Some(PrepCacheKey::Plain(head));
    }
    // 快照路径：`sync_policy: none` 时 `resolve_instance` 先于链行走返回
    // 快照（`data.snapshot`），master 后改不影响它 ⇒ 只跟自身。
    // 无快照时 `resolve_instance` 返回 `None` ⇒ 不缓存。
    if object.data.get("sync_policy").and_then(Value::as_str) == Some("none") {
        if object.data.get("snapshot").is_some() {
            return Some(PrepCacheKey::InstanceSnapshot(head));
        }
        return None;
    }
    let mut chain = Vec::new();
    let mut cursor: &Object = object;
    for _ in 0..64 {
        let master_id = cursor
            .data
            .get("master_ref")
            .and_then(|master_ref| master_ref.get("object_id"))
            .and_then(Value::as_str)?;
        let master = state.objects.get(master_id)?;
        if master.is_deleted() || master.current_version.is_none() {
            return None;
        }
        chain.push(link_of(master));
        if master.object_type != ObjectType::Instance {
            return Some(PrepCacheKey::InstanceChain { head, chain });
        }
        cursor = master;
    }
    None
}

/// 现算一条前置条目：**逐行复现今日循环的预处理段**（解析＋变换＋包围盒），
/// 不改任何语义 —— 缓存填充的成本就是今日成本，命中的成本是零。
pub fn compute_prep_entry(state: &DocumentState, object: &Object) -> PrepEntry {
    let primitive = if object.object_type == ObjectType::Instance {
        match resolve_instance(state, object) {
            Some((master_primitive, transform)) => {
                transform_primitive(master_primitive, &transform)
            }
            // **master 不可用（不存在/已删/成环）⇒ 什么都不画** ✓，
            // 但**不报错** ✓：日志顺序允许先建实例后建 master ✓，补齐后自动恢复 ✓。
            None => Primitive::Unsupported {
                reason: "实例的 master 当前不可用".to_owned(),
            },
        }
    } else {
        transform_primitive(parse_object(object), &object.transform)
    };
    let affects_whole_layer = matches!(
        primitive,
        Primitive::Adjustment { .. } | Primitive::Filter { .. } | Primitive::Unsupported { .. }
    );
    let bbox = if affects_whole_layer {
        None
    } else {
        object_bbox_in(state, object)
    };
    PrepEntry { primitive, bbox }
}
