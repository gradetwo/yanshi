//! 广播策略：控制流与数据流（设计文档 6.8 / 12.8）。
//!
//! | 类型 | 内容 | 广播策略 |
//! |---|---|---|
//! | **控制流** | **所有原子元数据**（全部 kind） | **全局广播** |
//! | **数据流** | Tile 位图、缩略图位图、blob 二进制 | 视口订阅过滤 / 按需拉取 |
//!
//! 划界依据：客户端折叠状态需要**全部**原子元数据——创建类原子同样影响对象树，
//! 漏收即状态不一致。原子元数据很小（大二进制已外置至 CAS），全量广播成本可忽略。
//!
//! 通道分工（6.7）：MCP stdio 走「提交 + 轮询」，不做推送；WS 推送只服务 Web 客户端。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use yanshi_core::{Atom, AtomKind, Bbox, Seq, SessionId};
use yanshi_render::thumb::ThumbKind;
use yanshi_render::tile::TileKey;

/// 订阅者标识。
pub type SubscriberId = u64;

/// 单个订阅者队列的**数据流**上限（事件数）。
///
/// 超过后**只丢数据流**（tile / 缩略图）：它们可以按需重取；
/// **控制流（原子）绝不因它丢**——丢了客户端折叠状态会与服务端静默不一致。
pub const QUEUE_DATA_LIMIT: usize = 256;

/// 单个订阅者队列的**硬上限**（事件数）。
///
/// 队列被**控制流**占满（已挤不出可丢弃的数据流）⇒ 该订阅者已被判定为「读不动」⇒
/// 标记溢出并由推送线程断开连接（客户端重连后重新同步）。上限本身把慢客户端的
/// 内存占用钉住（审计 P1-3）。
pub const QUEUE_HARD_LIMIT: usize = 1024;

fn is_data_event(event: &BroadcastEvent) -> bool {
    matches!(
        event,
        BroadcastEvent::Tiles { .. } | BroadcastEvent::Thumbnail { .. }
    )
}

/// 推送通道类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushChannel {
    /// WebSocket：接收控制流与视口内的数据流。
    WebSocket,
    /// MCP stdio：不接收推送，改为轮询（`get_log` / `get_render_status`）。
    Poll,
    /// 进程内订阅（测试与嵌入式）。
    InProcess,
}

impl PushChannel {
    /// 是否接收推送。
    pub const fn accepts_push(self) -> bool {
        !matches!(self, Self::Poll)
    }
}

/// 订阅者。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subscriber {
    /// 标识。
    pub id: SubscriberId,
    /// 会话。
    pub session: SessionId,
    /// 通道。
    pub channel: PushChannel,
    /// 视口（文档坐标），仅影响数据流。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Bbox>,
    /// 缩放级别。
    #[serde(default = "default_zoom")]
    pub zoom: f64,
}

fn default_zoom() -> f64 {
    1.0
}

impl Broadcaster {
    /// **★ 所有订阅者视口的**并集**（视口淘汰要用 ✓，2026-10-15 ✓）★**
    ///
    /// **★ 保守规则 ★**：**只要有一个订阅者没声明视口（`None`）⇒ 返回 `None`** ✓；
    /// **零订阅者**也返回 `None` ✓（没有信息 ⇒ 不淘汰 ✓，安全优先 ✓）
    /// ⇒ ⇒ 调用处的语义因此只有两种 ✓：`Some(vp)` ⇒ 按它淘汰 ✓；`None` ⇒ **什么都不淘汰** ✓。
    ///
    /// **为什么必须这样** ✗：推送过滤那边对 `None` 的语义是「**未声明视口：按需全量**」✓
    /// ⇒ 淘汰**必须与它一致** ✓ —— 否则"你以为没人看"的那部分，
    /// 恰恰是那个**没声明视口**的人要的 ✗（`docs/design/vermeer-acceptance.md` §49 之补二十四 ✓）。
    ///
    /// **为什么用并集而不是"某一个订阅者"** ✗：按单视口淘汰会把**别人还要用的** tile 扔掉 ✗
    /// —— 那正是判据报的「平移回来之后画面变了 ✗」，也是 `a152e034` 弄红 8 条的原因 ✓。
    pub fn viewport_union(&self) -> Option<Bbox> {
        let mut union: Option<Bbox> = None;
        for subscriber in self.subscribers.values() {
            // **有一个 `None` ⇒ 整体 `None`** ✓（保守规则 ✓，见上面文档 ✓）。
            let viewport = subscriber.viewport?;
            union = Some(match union {
                // **复用已有的** `Bbox::union` ✓（`yanshi-core/src/conflict.rs:70` ✓）。
                Some(current) => current.union(&viewport),
                None => viewport,
            });
        }
        union
    }
}

/// 广播事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum BroadcastEvent {
    /// 控制流：原子元数据（大二进制已外置至 CAS，原子本体体积可忽略）。
    ///
    /// 这里携带**完整原子**而非仅摘要：客户端折叠状态需要全部原子（含创建类与
    /// 载荷里的对象 id / 点列 / 颜色），只发 id/kind 会让客户端与服务端状态不一致
    /// （6.8 的划界依据）。位图仍然不在广播里，客户端按需从 CAS 拉取。
    Atom {
        /// 原子 id。
        atom_id: String,
        /// 权威序号。
        seq: Seq,
        /// 原子类型。
        kind: AtomKind,
        /// 操作者。
        actor: String,
        /// 会话。
        session: SessionId,
        /// 是否重型原子（触发快照）。
        heavy: bool,
        /// 完整原子（客户端本地折叠用；blob 已外置）。
        atom: Box<Atom>,
    },
    /// 数据流：tile 更新（已按视口过滤）。
    Tiles {
        /// tile 边长。
        tile_size: u32,
        /// 失效/更新过的 tile。
        keys: Vec<TileKey>,
    },
    /// 数据流：缩略图更新。
    Thumbnail {
        /// 缩略图类型。
        kind: ThumbKind,
    },
    /// job 完成事件（只服务 Web 客户端）。
    JobFinished {
        /// job id。
        job_id: String,
        /// 终态。
        status: String,
    },
    /// 标注通道有新内容（Web 客户端提示）。
    Annotation {
        /// 待处理标注数。
        pending: usize,
    },
}

/// 一次投递。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Delivery {
    /// 接收者。
    pub subscriber: SubscriberId,
    /// 事件。
    pub event: BroadcastEvent,
}

/// 广播统计（设计文档 14.9）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BroadcastStats {
    /// 控制流事件数。
    pub control_events: u64,
    /// 控制流投递次数。
    pub control_deliveries: u64,
    /// 数据流事件数。
    pub data_events: u64,
    /// 数据流投递次数（已通过视口过滤）。
    pub data_deliveries: u64,
    /// 被视口过滤掉的 tile 投递次数。
    pub data_filtered: u64,
    /// 因通道不支持推送而改走轮询的次数。
    pub polled_instead: u64,
    /// 队列越过数据流上限后被丢弃的数据流事件数（控制流不受影响）。
    pub queue_dropped_data: u64,
    /// 队列越过硬上限后被丢弃的事件数（此后该连接会被断开）。
    pub queue_dropped_overflow: u64,
}

/// 广播器。
#[derive(Debug, Default)]
pub struct Broadcaster {
    subscribers: BTreeMap<SubscriberId, Subscriber>,
    queues: BTreeMap<SubscriberId, Vec<BroadcastEvent>>,
    overflowed: BTreeSet<SubscriberId>,
    next_id: SubscriberId,
    stats: BroadcastStats,
}

impl Broadcaster {
    /// 空广播器。
    pub fn new() -> Self {
        Self {
            next_id: 1,
            ..Self::default()
        }
    }

    /// 订阅。
    pub fn subscribe(
        &mut self,
        session: impl Into<SessionId>,
        channel: PushChannel,
    ) -> SubscriberId {
        let id = self.next_id;
        self.next_id += 1;
        self.subscribers.insert(
            id,
            Subscriber {
                id,
                session: session.into(),
                channel,
                viewport: None,
                zoom: 1.0,
            },
        );
        self.queues.insert(id, Vec::new());
        id
    }

    /// 退订。
    pub fn unsubscribe(&mut self, id: SubscriberId) -> bool {
        self.queues.remove(&id);
        self.overflowed.remove(&id);
        self.subscribers.remove(&id).is_some()
    }

    /// 订阅者数。
    pub fn len(&self) -> usize {
        self.subscribers.len()
    }

    /// 是否没有订阅者。
    pub fn is_empty(&self) -> bool {
        self.subscribers.is_empty()
    }

    /// 设置视口（WS 的 viewport/zoom 订阅消息）。
    pub fn set_viewport(&mut self, id: SubscriberId, viewport: Option<Bbox>, zoom: f64) -> bool {
        match self.subscribers.get_mut(&id) {
            Some(subscriber) => {
                subscriber.viewport = viewport;
                subscriber.zoom = zoom;
                true
            }
            None => false,
        }
    }

    /// 查询订阅者。
    pub fn subscriber(&self, id: SubscriberId) -> Option<&Subscriber> {
        self.subscribers.get(&id)
    }

    /// 统计。
    pub const fn stats(&self) -> BroadcastStats {
        self.stats
    }

    /// 控制流：把原子元数据广播给所有接收推送的订阅者。
    ///
    /// 不按视口过滤——漏收创建类原子会让客户端折叠状态与权威状态不一致。
    pub fn publish_atom(&mut self, atom: &Atom) -> Vec<Delivery> {
        let event = BroadcastEvent::Atom {
            atom_id: atom.id.clone(),
            seq: atom.seq,
            kind: atom.kind,
            actor: atom.actor.clone(),
            session: atom.session.clone(),
            heavy: atom.is_heavy(),
            atom: Box::new(atom.clone()),
        };
        self.stats.control_events += 1;
        let deliveries = self.deliver(event.clone());
        self.stats.control_deliveries += deliveries.len() as u64;
        let polling = self
            .subscribers
            .values()
            .filter(|subscriber| !subscriber.channel.accepts_push())
            .count() as u64;
        self.stats.polled_instead += polling;
        deliveries
    }

    /// 数据流：tile 更新，仅推给视口相交的接收推送的订阅者。
    pub fn publish_tiles(&mut self, tile_size: u32, keys: &[TileKey]) -> Vec<Delivery> {
        let event = BroadcastEvent::Tiles {
            tile_size,
            keys: keys.to_vec(),
        };
        self.stats.data_events += 1;
        let mut deliveries = Vec::new();
        let targets: Vec<SubscriberId> = self
            .subscribers
            .values()
            .filter(|subscriber| subscriber.channel.accepts_push())
            .map(|subscriber| subscriber.id)
            .collect();
        for id in targets {
            let visible: Vec<TileKey> = keys
                .iter()
                .copied()
                .filter(|key| {
                    let viewport = self
                        .subscribers
                        .get(&id)
                        .and_then(|subscriber| subscriber.viewport);
                    match viewport {
                        Some(viewport) => tile_bounds(tile_size, *key).intersects(&viewport),
                        None => true, // 未声明视口：按需全量（客户端自行缓存）
                    }
                })
                .collect();
            if visible.is_empty() {
                // 视口与该批 tile 完全不相交：整批过滤。
                self.stats.data_filtered += keys.len() as u64;
                continue;
            }
            self.stats.data_filtered += (keys.len() - visible.len()) as u64;
            let filtered = if visible.len() == keys.len() {
                event.clone()
            } else {
                BroadcastEvent::Tiles {
                    tile_size,
                    keys: visible,
                }
            };
            if self.enqueue(id, filtered.clone()) {
                deliveries.push(Delivery {
                    subscriber: id,
                    event: filtered,
                });
            }
        }
        self.stats.data_deliveries += deliveries.len() as u64;
        deliveries
    }

    /// 数据流：缩略图更新。
    pub fn publish_thumbnail(&mut self, kind: ThumbKind) -> Vec<Delivery> {
        self.stats.data_events += 1;
        let deliveries = self.deliver(BroadcastEvent::Thumbnail { kind });
        self.stats.data_deliveries += deliveries.len() as u64;
        deliveries
    }

    /// 事件：job 完成（只服务 Web 客户端）。
    pub fn publish_job_finished(
        &mut self,
        job_id: impl Into<String>,
        status: &str,
    ) -> Vec<Delivery> {
        self.deliver(BroadcastEvent::JobFinished {
            job_id: job_id.into(),
            status: status.to_owned(),
        })
    }

    /// 事件：标注通道有新内容。
    pub fn publish_annotations(&mut self, pending: usize) -> Vec<Delivery> {
        self.deliver(BroadcastEvent::Annotation { pending })
    }

    /// 取出某个订阅者排队的事件（WS 写出队列）。
    pub fn drain(&mut self, id: SubscriberId) -> Vec<BroadcastEvent> {
        self.queues
            .get_mut(&id)
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// 队列中待发送的事件数。
    pub fn queued(&self, id: SubscriberId) -> usize {
        self.queues.get(&id).map(|queue| queue.len()).unwrap_or(0)
    }

    /// 取出并清除「该订阅者已溢出」标记（推送线程据此断开连接）。
    pub fn take_overflowed(&mut self, id: SubscriberId) -> bool {
        self.overflowed.remove(&id)
    }

    /// 入队；返回是否真的排进去了（被上限丢掉的事件不计入投递）。
    ///
    /// **策略**（审计 P1-3）：数据流（tile / 缩略图）先丢 ✓ —— 客户端可以按需重取 ✓；
    /// **控制流不丢** ✗ —— 丢一个原子会让客户端折叠状态与服务端静默不一致 ✗。
    /// 硬上限被控制流占满时，先**挤掉一条更旧的数据流**再入队 ✓；
    /// 连一条数据流都挤不出去（队列里全是原子）⇒ 标记溢出、由推送线程断开连接 ✓。
    fn enqueue(&mut self, id: SubscriberId, event: BroadcastEvent) -> bool {
        let Some(queue) = self.queues.get_mut(&id) else {
            return false;
        };
        if self.overflowed.contains(&id) {
            self.stats.queue_dropped_overflow += 1;
            return false;
        }
        if is_data_event(&event) {
            if queue.len() >= QUEUE_DATA_LIMIT {
                self.stats.queue_dropped_data += 1;
                return false;
            }
            queue.push(event);
            return true;
        }
        if queue.len() >= QUEUE_HARD_LIMIT {
            let oldest_data = queue.iter().position(is_data_event);
            match oldest_data {
                Some(position) => {
                    queue.remove(position);
                    self.stats.queue_dropped_data += 1;
                }
                None => {
                    self.overflowed.insert(id);
                    self.stats.queue_dropped_overflow += 1;
                    return false;
                }
            }
        }
        queue.push(event);
        true
    }

    fn deliver(&mut self, event: BroadcastEvent) -> Vec<Delivery> {
        let targets: Vec<SubscriberId> = self
            .subscribers
            .values()
            .filter(|subscriber| subscriber.channel.accepts_push())
            .map(|subscriber| subscriber.id)
            .collect();
        let mut deliveries = Vec::with_capacity(targets.len());
        for id in targets {
            if self.enqueue(id, event.clone()) {
                deliveries.push(Delivery {
                    subscriber: id,
                    event: event.clone(),
                });
            }
        }
        deliveries
    }
}

/// tile 在文档坐标下的边界（广播过滤用；与 `TileGrid::bounds` 同构）。
pub fn tile_bounds(tile_size: u32, key: TileKey) -> Bbox {
    Bbox::new(
        (key.x * tile_size) as f64,
        (key.y * tile_size) as f64,
        tile_size as f64,
        tile_size as f64,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn atom(kind: AtomKind, seq: Seq, id: &str) -> Atom {
        let mut atom = Atom::new(kind, "human:1", "session:a", json!({})).with_id(id);
        atom.seq = seq;
        atom
    }

    /// **★ `viewport_union` 的**保守规则**（2026-10-15 ✓）★**
    ///
    /// 这条规则是整个视口淘汰设计的**要害** ✗✓：`None`（有人没声明视口）的语义是
    /// 「**未声明视口：按需全量**」✓ ⇒ 此时**谁都不许淘汰** ✓
    /// ⇒ 否则"你以为没人看"的那部分，恰恰是那个没声明视口的人要的 ✗（§49 之补二十四 ✓）。
    #[test]
    fn viewport_union_is_conservative() {
        let mut broadcaster = Broadcaster::new();
        // ① **零订阅者** ⇒ None ✓（没有信息 ⇒ 不淘汰 ✓）
        assert!(
            broadcaster.viewport_union().is_none(),
            "没有观看者 ⇒ 必须是 None ✓（不淘汰 ✓）"
        );
        // ② **一个**声明了视口 ⇒ Some(它) ✓
        let a = broadcaster.subscribe("session:a", PushChannel::WebSocket);
        broadcaster.set_viewport(a, Some(Bbox::new(0.0, 0.0, 10.0, 10.0)), 1.0);
        let one = broadcaster
            .viewport_union()
            .expect("有视口 ⇒ 应当是 Some ✓");
        assert!(
            one.x == 0.0 && one.y == 0.0 && one.w == 10.0 && one.h == 10.0,
            "单个观看者 ⇒ 并集就是它本身 ✓（实测 {one:?} ✗）"
        );
        // ③ **两个都声明** ⇒ **并集** ✓（必须同时覆盖两块 ✓）
        let b = broadcaster.subscribe("session:b", PushChannel::WebSocket);
        broadcaster.set_viewport(b, Some(Bbox::new(20.0, 30.0, 10.0, 10.0)), 1.0);
        let both = broadcaster
            .viewport_union()
            .expect("都有视口 ⇒ 应当是 Some ✓");
        assert!(
            both.x <= 0.0 && both.y <= 0.0 && both.x + both.w >= 30.0 && both.y + both.h >= 40.0,
            "并集必须**同时覆盖**两块 ✓（实测 {both:?} ✗）"
        );
        // ④ ★ **保守规则**：再来一个**没声明视口**的 ⇒ 必须变回 None ✓ ★
        let c = broadcaster.subscribe("session:c", PushChannel::WebSocket);
        assert!(
            broadcaster.viewport_union().is_none(),
            "**有人没声明视口 ⇒ 必须 None** ✓（那是「按需全量」✓ ⇒ 谁都不许淘汰 ✓）"
        );
        // ⑤ 他一旦声明 ⇒ 又回到并集 ✓（证明上一条不是"永远 None" ✗）
        broadcaster.set_viewport(c, Some(Bbox::new(-5.0, -5.0, 1.0, 1.0)), 1.0);
        let again = broadcaster.viewport_union().expect("三人都声明 ⇒ Some ✓");
        assert!(
            again.x <= -5.0 && again.y <= -5.0 && again.x + again.w >= 30.0,
            "第三人加入后并集要**变大** ✓（实测 {again:?} ✗）"
        );
    }

    #[test]
    fn control_flow_reaches_every_pushing_subscriber_regardless_of_viewport() {
        let mut broadcaster = Broadcaster::new();
        let web = broadcaster.subscribe("session:web", PushChannel::WebSocket);
        let other = broadcaster.subscribe("session:web2", PushChannel::WebSocket);
        broadcaster.set_viewport(web, Some(Bbox::new(0.0, 0.0, 10.0, 10.0)), 1.0);
        broadcaster.set_viewport(other, Some(Bbox::new(900.0, 900.0, 10.0, 10.0)), 1.0);

        let deliveries = broadcaster.publish_atom(&atom(AtomKind::CreateLayer, 1, "a1"));
        assert_eq!(deliveries.len(), 2, "创建类原子必须全局广播");
        for subscriber in [web, other] {
            let queued = broadcaster.drain(subscriber);
            assert_eq!(queued.len(), 1);
            match &queued[0] {
                BroadcastEvent::Atom { kind, seq, .. } => {
                    assert_eq!(*kind, AtomKind::CreateLayer);
                    assert_eq!(*seq, 1);
                }
                other => panic!("期望 Atom 事件，得到 {other:?}"),
            }
        }
        let stats = broadcaster.stats();
        assert_eq!(stats.control_events, 1);
        assert_eq!(stats.control_deliveries, 2);
    }

    #[test]
    fn poll_channels_receive_no_pushes() {
        let mut broadcaster = Broadcaster::new();
        let stdio = broadcaster.subscribe("session:mcp", PushChannel::Poll);
        let web = broadcaster.subscribe("session:web", PushChannel::WebSocket);

        broadcaster.publish_atom(&atom(AtomKind::DrawStroke, 2, "a2"));
        assert_eq!(broadcaster.queued(stdio), 0, "MCP stdio 不做推送");
        assert_eq!(broadcaster.queued(web), 1);
        assert_eq!(broadcaster.stats().polled_instead, 1);
    }

    #[test]
    fn data_flow_is_filtered_by_viewport() {
        let mut broadcaster = Broadcaster::new();
        let near = broadcaster.subscribe("session:a", PushChannel::WebSocket);
        let far = broadcaster.subscribe("session:b", PushChannel::WebSocket);
        let no_viewport = broadcaster.subscribe("session:c", PushChannel::WebSocket);
        // 视口覆盖 512×256 → 恰好包含 tile (0,0) 与 (1,0)。
        broadcaster.set_viewport(near, Some(Bbox::new(0.0, 0.0, 512.0, 256.0)), 1.0);
        broadcaster.set_viewport(far, Some(Bbox::new(8192.0, 8192.0, 64.0, 64.0)), 1.0);

        let keys = vec![TileKey::new(0, 0), TileKey::new(1, 0), TileKey::new(64, 64)];
        let deliveries = broadcaster.publish_tiles(256, &keys);
        // near 收到 (0,0) 与 (1,0)；far 全部被过滤；无视口者按需全量。
        assert_eq!(deliveries.len(), 2);
        let near_events = broadcaster.drain(near);
        match &near_events[0] {
            BroadcastEvent::Tiles { keys, .. } => {
                assert_eq!(keys, &vec![TileKey::new(0, 0), TileKey::new(1, 0)]);
            }
            other => panic!("期望 Tiles，得到 {other:?}"),
        }
        assert!(broadcaster.drain(far).is_empty(), "视口外不推送数据流");
        assert_eq!(broadcaster.drain(no_viewport).len(), 1);
        let stats = broadcaster.stats();
        // far 的 3 个 tile 全部被过滤；near 的 (64,64) 也被过滤。
        assert_eq!(stats.data_filtered, 4);
        assert_eq!(stats.data_deliveries, 2);
    }

    #[test]
    fn unsubscribe_stops_delivery() {
        let mut broadcaster = Broadcaster::new();
        let id = broadcaster.subscribe("s", PushChannel::WebSocket);
        assert!(broadcaster.unsubscribe(id));
        assert!(broadcaster
            .publish_atom(&atom(AtomKind::Comment, 3, "a3"))
            .is_empty());
        assert!(broadcaster.is_empty());
        assert!(!broadcaster.unsubscribe(id));
    }

    #[test]
    fn job_and_annotation_events_are_delivered() {
        let mut broadcaster = Broadcaster::new();
        let web = broadcaster.subscribe("s", PushChannel::WebSocket);
        broadcaster.publish_job_finished("job_1", "committed");
        broadcaster.publish_annotations(2);
        broadcaster.publish_thumbnail(ThumbKind::Doc128);
        let events = broadcaster.drain(web);
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], BroadcastEvent::JobFinished { .. }));
        assert!(matches!(
            events[1],
            BroadcastEvent::Annotation { pending: 2 }
        ));
        assert!(matches!(events[2], BroadcastEvent::Thumbnail { .. }));
    }

    /// **慢客户端（审计 P1-3）**：数据流排队到上限就被丢，**控制流不丢**。
    ///
    /// **为什么控制流不能丢** ✗：客户端折叠状态靠全部原子 ✓ ⇒ 丢一个原子会让
    /// 本地状态与服务端**静默**不一致 ✗（数据流丢了可以按需重取 ✓）。
    #[test]
    fn queue_drops_data_flow_before_control_flow() {
        let mut broadcaster = Broadcaster::new();
        let web = broadcaster.subscribe("s", PushChannel::WebSocket);
        let keys = vec![TileKey::new(0, 0)];
        let overshoot = 10;
        for _ in 0..QUEUE_DATA_LIMIT + overshoot {
            broadcaster.publish_tiles(256, &keys);
        }
        assert_eq!(
            broadcaster.queued(web),
            QUEUE_DATA_LIMIT,
            "数据流排队不得超过软上限"
        );
        assert_eq!(
            broadcaster.stats().queue_dropped_data,
            overshoot as u64,
            "超出的数据流事件必须被**计数**（不许静默丢）"
        );
        assert!(
            !broadcaster.take_overflowed(web),
            "只丢数据流不算溢出（连接不该被断）"
        );
        // 软上限之后控制流仍然入队 ✓（这是"只丢数据流"的证据 ✓）。
        broadcaster.publish_atom(&atom(AtomKind::Comment, 1, "a1"));
        assert_eq!(broadcaster.queued(web), QUEUE_DATA_LIMIT + 1);
        let drained = broadcaster.drain(web);
        assert!(
            matches!(drained.last(), Some(BroadcastEvent::Atom { .. })),
            "控制流必须排在队尾、不许被丢"
        );
    }

    /// **慢客户端（审计 P1-3）**：连控制流也顶到硬上限 ⇒ 标记溢出（推送线程据此断开）。
    #[test]
    fn queue_overflow_flags_the_subscriber() {
        let mut broadcaster = Broadcaster::new();
        let web = broadcaster.subscribe("s", PushChannel::WebSocket);
        let overshoot = 5;
        for seq in 0..QUEUE_HARD_LIMIT + overshoot {
            broadcaster.publish_job_finished(format!("job_{seq}"), "committed");
        }
        assert_eq!(
            broadcaster.queued(web),
            QUEUE_HARD_LIMIT,
            "硬上限必须把队列钉住（内存 DoS 的边界）"
        );
        assert_eq!(
            broadcaster.stats().queue_dropped_overflow,
            overshoot as u64,
            "越过硬上限的每个事件都要计数"
        );
        assert!(broadcaster.take_overflowed(web), "越过硬上限必须标记溢出");
        assert!(!broadcaster.take_overflowed(web), "标记只能取一次");
        broadcaster.publish_job_finished("job_after", "committed");
        assert_eq!(
            broadcaster.queued(web),
            QUEUE_HARD_LIMIT,
            "已溢出后不再入队（等推送线程断开该连接）"
        );
    }

    /// **控制流不丢**（审计 P1-3）：硬上限被占满时，先挤掉更旧的数据流再入队。
    #[test]
    fn control_flow_evicts_queued_data_before_overflowing() {
        let mut broadcaster = Broadcaster::new();
        let web = broadcaster.subscribe("s", PushChannel::WebSocket);
        let keys = vec![TileKey::new(0, 0)];
        for _ in 0..QUEUE_DATA_LIMIT {
            broadcaster.publish_tiles(256, &keys);
        }
        // 再灌控制流把队列正好填到硬上限（256 条数据流 + 768 条控制流）。
        for seq in 0..(QUEUE_HARD_LIMIT - QUEUE_DATA_LIMIT) {
            broadcaster.publish_job_finished(format!("job_{seq}"), "committed");
        }
        assert_eq!(broadcaster.queued(web), QUEUE_HARD_LIMIT);
        // 再来一条控制流 ⇒ 挤掉一条数据流（不是溢出 ✓）。
        broadcaster.publish_job_finished("job_evict", "committed");
        assert_eq!(broadcaster.queued(web), QUEUE_HARD_LIMIT, "上限仍被钉住");
        assert_eq!(
            broadcaster.stats().queue_dropped_data,
            1,
            "被挤掉的是数据流（可重取），不是控制流"
        );
        assert!(
            !broadcaster.take_overflowed(web),
            "队列里还有数据流可挤 ⇒ 不算溢出"
        );
        // 把剩下的数据流全部挤掉仍未溢出；再挤不出来时才算溢出。
        for seq in 0..QUEUE_DATA_LIMIT - 1 {
            broadcaster.publish_job_finished(format!("job_more_{seq}"), "committed");
        }
        assert!(!broadcaster.take_overflowed(web), "还有数据流可挤");
        broadcaster.publish_job_finished("job_last", "committed");
        assert!(
            broadcaster.take_overflowed(web),
            "队列全是原子 ⇒ 读不动的证据 ⇒ 必须溢出"
        );
    }

    /// 退订要同时清掉溢出标记（否则同一 id 的残留状态会误伤后续判断）。
    #[test]
    fn unsubscribe_clears_overflow_flag() {
        let mut broadcaster = Broadcaster::new();
        let web = broadcaster.subscribe("s", PushChannel::WebSocket);
        for seq in 0..QUEUE_HARD_LIMIT + 1 {
            broadcaster.publish_job_finished(format!("job_{seq}"), "committed");
        }
        assert!(broadcaster.unsubscribe(web));
        assert!(
            !broadcaster.take_overflowed(web),
            "退订必须清掉溢出标记（否则标记会留在集合里）"
        );
    }
}
