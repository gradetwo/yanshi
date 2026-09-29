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
use std::collections::BTreeMap;
use yanshi_core::{Atom, AtomKind, Bbox, Seq, SessionId};
use yanshi_render::thumb::ThumbKind;
use yanshi_render::tile::TileKey;

/// 订阅者标识。
pub type SubscriberId = u64;

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

/// 广播事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum BroadcastEvent {
    /// 控制流：原子元数据（大二进制已外置，体积可忽略）。
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
}

/// 广播器。
#[derive(Debug, Default)]
pub struct Broadcaster {
    subscribers: BTreeMap<SubscriberId, Subscriber>,
    queues: BTreeMap<SubscriberId, Vec<BroadcastEvent>>,
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
            self.queues.entry(id).or_default().push(filtered.clone());
            deliveries.push(Delivery {
                subscriber: id,
                event: filtered,
            });
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

    fn deliver(&mut self, event: BroadcastEvent) -> Vec<Delivery> {
        let targets: Vec<SubscriberId> = self
            .subscribers
            .values()
            .filter(|subscriber| subscriber.channel.accepts_push())
            .map(|subscriber| subscriber.id)
            .collect();
        let mut deliveries = Vec::with_capacity(targets.len());
        for id in targets {
            self.queues.entry(id).or_default().push(event.clone());
            deliveries.push(Delivery {
                subscriber: id,
                event: event.clone(),
            });
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
}
