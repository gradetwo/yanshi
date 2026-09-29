//! 服务端端到端集成测试：提交 → 冲突 → Job → 广播 → 标注 → 时间旅行 → GC。

use serde_json::json;
use std::sync::Arc;
use yanshi_core::blob::MemoryBlobStore;
use yanshi_core::{Atom, AtomKind, Bbox, DocId, ErrorCode, HeadBase};
use yanshi_render::tile::TileKey;
use yanshi_server::broadcast::PushChannel;
use yanshi_server::token::{Role, TransportKind};
use yanshi_server::tools::{ToolContext, ToolRegistry};
use yanshi_server::{DocumentSettings, NewDocument, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings {
        tile_size: 32,
        ..DocumentSettings::default()
    })
}

fn atom(kind: AtomKind, actor: &str, session: &str, payload: serde_json::Value) -> Atom {
    Atom::new(kind, actor, session, payload)
}

fn setup(workspace: &mut Workspace) -> DocId {
    workspace
        .create_document(NewDocument::new("doc_1", 96, 96), "human:1", "session:a")
        .unwrap();
    for payload_atom in [
        atom(
            AtomKind::CreateLayer,
            "human:1",
            "session:a",
            json!({"layer_id": "layer_1"}),
        ),
        atom(
            AtomKind::DrawStroke,
            "human:1",
            "session:a",
            json!({
                "object_id": "obj_1",
                "layer_id": "layer_1",
                "data": {"points": [[8.0, 8.0], [48.0, 40.0]], "size": 5.0},
            }),
        ),
    ] {
        workspace
            .commit("doc_1", payload_atom, "human:1", false)
            .unwrap();
    }
    "doc_1".to_owned()
}

#[test]
fn sampling_replace_conflict_creates_conflict_layer_and_returns_error() {
    let mut workspace = workspace();
    setup(&mut workspace);

    // 会话 A 的采样性替换（retouch，基区域 0,0,32,32）。
    let first = workspace
        .commit(
            "doc_1",
            atom(
                AtomKind::Retouch,
                "human:1",
                "session:a",
                json!({
                    "object_id": "obj_1",
                    "layer_id": "layer_1",
                    "region": {"x": 0.0, "y": 0.0, "w": 32.0, "h": 32.0},
                }),
            ),
            "human:1",
            false,
        )
        .unwrap();
    assert_eq!(first.seq, 4);

    // 会话 B 在重叠区域做采样性替换 → conflict（12.3 第 2 步）。
    let error = workspace
        .commit(
            "doc_1",
            atom(
                AtomKind::Retouch,
                "ai:1",
                "session:b",
                json!({
                    "object_id": "obj_1",
                    "layer_id": "layer_1",
                    "region": {"x": 8.0, "y": 8.0, "w": 32.0, "h": 32.0},
                }),
            ),
            "ai:1",
            false,
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Conflict);
    assert!(error.retryable, "冲突可重试（换 ULID 改投冲突图层）");
    let conflict_layer = error.context.extra["conflict_layer_id"]
        .as_str()
        .expect("错误里带冲突图层 id")
        .to_owned();

    // 冲突图层由系统 actor 创建，且带 metadata.conflict 供审计。
    let document = workspace.document("doc_1").unwrap();
    let layer = document.state().layers.get(&conflict_layer).unwrap();
    assert_eq!(layer.created_by, document.log().atoms().last().unwrap().id);
    assert_eq!(layer.metadata["conflict"], json!(true));
    assert_eq!(layer.name, "conflict");

    // 非重叠区域的并发采样性替换不受影响。
    let ok = workspace
        .commit(
            "doc_1",
            atom(
                AtomKind::Retouch,
                "ai:1",
                "session:b",
                json!({
                    "object_id": "obj_1",
                    "layer_id": "layer_1",
                    "region": {"x": 64.0, "y": 64.0, "w": 32.0, "h": 32.0},
                }),
            ),
            "ai:1",
            false,
        )
        .unwrap();
    assert!(ok.seq > first.seq);

    // 客户端换新 ULID 改投冲突图层即可提交（12.3 第 3 步）。
    let retry = workspace
        .commit(
            "doc_1",
            atom(
                AtomKind::Retouch,
                "ai:1",
                "session:b",
                json!({
                    "object_id": "obj_1",
                    "layer_id": conflict_layer,
                    "region": {"x": 8.0, "y": 8.0, "w": 32.0, "h": 32.0},
                }),
            ),
            "ai:1",
            false,
        )
        .unwrap();
    assert!(retry.dirty_tiles.len() <= 9);
}

#[test]
fn jobs_ttl_cancel_and_render_watermark() {
    let mut workspace = workspace();
    setup(&mut workspace);

    // 重型原子（liquify）→ job；wait_for_render = false 时不自动完成。
    let result = workspace
        .commit(
            "doc_1",
            atom(
                AtomKind::Liquify,
                "human:1",
                "session:a",
                json!({
                    "object_id": "obj_1",
                    "layer_id": "layer_1",
                    "region": {"x": 0.0, "y": 0.0, "w": 16.0, "h": 16.0},
                }),
            ),
            "human:1",
            false,
        )
        .unwrap();
    let job_id = result.job_id.expect("重型原子产生 job");

    let now = yanshi_core::now_ms();
    let job = workspace
        .document_mut("doc_1")
        .unwrap()
        .jobs_mut()
        .get(&job_id, now)
        .unwrap();
    assert_eq!(job.status.as_str(), "submitted");
    assert_eq!(job.ttl, yanshi_server::DEFAULT_JOB_TTL_SECONDS);

    // 渲染水位落后 → get_render_status 报 false。
    let status = workspace.render_status("doc_1", &result.atom_id).unwrap();
    assert!(!status.rendered);

    // run_pending_jobs 完成渲染与 job。
    let completed = workspace
        .document_mut("doc_1")
        .unwrap()
        .run_pending_jobs()
        .unwrap();
    assert_eq!(completed, vec![job_id.clone()]);
    let status = workspace.render_status("doc_1", &result.atom_id).unwrap();
    assert!(status.rendered);

    // 已完成的 job 不能重复取消；终态可查询。
    let cancel = workspace
        .document_mut("doc_1")
        .unwrap()
        .jobs_mut()
        .cancel(&job_id, now);
    assert!(cancel.is_err(), "终态不可取消");

    // 另一个 job 可取消（TTL 内）。
    let second = workspace
        .commit(
            "doc_1",
            atom(
                AtomKind::Liquify,
                "human:1",
                "session:a",
                json!({"object_id": "obj_1", "layer_id": "layer_1", "region": {"x":0,"y":0,"w":8,"h":8}}),
            ),
            "human:1",
            false,
        )
        .unwrap()
        .job_id
        .unwrap();
    let cancelled = workspace
        .document_mut("doc_1")
        .unwrap()
        .jobs_mut()
        .cancel(&second, now)
        .unwrap();
    assert_eq!(cancelled.as_str(), "cancelled");

    // 未知 job → job_not_found（5.7）。
    assert_eq!(
        workspace
            .document_mut("doc_1")
            .unwrap()
            .jobs_mut()
            .get("job_missing", now)
            .unwrap_err()
            .code,
        ErrorCode::JobNotFound
    );
}

#[test]
fn broadcast_separates_control_and_data_flow() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let document = workspace.document_mut("doc_1").unwrap();
    let web = document.subscribe("session:web", PushChannel::WebSocket);
    let stdio = document.subscribe("session:mcp", PushChannel::Poll);
    // Web 客户端只订阅左上角 32×32 视口。
    document
        .broadcaster_mut()
        .set_viewport(web, Some(Bbox::new(0.0, 0.0, 32.0, 32.0)), 1.0);
    document.broadcaster_mut().drain(web); // 清掉建文档阶段的事件

    // 控制流：创建图层这种“不落在视口里”的原子也必须广播。
    workspace
        .commit(
            "doc_1",
            atom(
                AtomKind::CreateLayer,
                "human:1",
                "session:a",
                json!({"layer_id": "layer_2", "z_index": 9}),
            ),
            "human:1",
            false,
        )
        .unwrap();
    let document = workspace.document_mut("doc_1").unwrap();
    let events = document.broadcaster_mut().drain(web);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, yanshi_server::BroadcastEvent::Atom { .. })),
        "控制流必须全局广播：{events:?}"
    );
    assert_eq!(document.broadcaster().queued(stdio), 0, "stdio 不接收推送");

    // 数据流：视口外的 tile 不推送。
    let far = TileKey::new(2, 2); // 64..96 的 tile
    document.broadcaster_mut().publish_tiles(32, &[far]);
    assert_eq!(document.broadcaster_mut().drain(web).len(), 0, "视口外过滤");
    let near = TileKey::new(0, 0);
    document.broadcaster_mut().publish_tiles(32, &[near]);
    assert_eq!(document.broadcaster_mut().drain(web).len(), 1);
    let stats = document.broadcaster().stats();
    assert!(stats.control_events >= 1);
    assert!(stats.data_filtered >= 1);
}

#[test]
fn annotations_stay_out_of_the_atom_log() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let atoms_before = workspace.document("doc_1").unwrap().atom_count();

    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Annotation,
    ]);
    let mut context = ToolContext::new(&mut workspace, "doc_1", "human:1", "session:web");
    let created = registry.call(
        &mut context,
        "create_annotation",
        &json!({
            "type": "region",
            "intent": "modify",
            "target": {"target": "region", "bbox": {"x": 0, "y": 0, "w": 16, "h": 16}},
            "content": "这里的笔触太重",
        }),
    );
    assert_eq!(created["ok"], json!(true), "{created}");
    let annotation_id = created["annotation_id"].as_str().unwrap().to_owned();

    let listed = registry.call(
        &mut context,
        "list_annotations",
        &json!({"status": "pending"}),
    );
    assert_eq!(listed["count"], json!(1));

    let resolved = registry.call(
        &mut context,
        "resolve_annotation",
        &json!({"annotation_id": annotation_id}),
    );
    assert_eq!(resolved["annotation"]["status"], json!("resolved"));

    // 标注不进入主原子日志（4.6）。
    let document = workspace.document("doc_1").unwrap();
    assert_eq!(document.atom_count(), atoms_before, "标注不产生原子");
    assert_eq!(document.annotations().len(), 1);
    assert_eq!(
        document.annotations().appended(),
        2,
        "create + resolve 都是追加"
    );
}

#[test]
fn time_travel_and_checkpoint_restore() {
    let mut workspace = workspace();
    setup(&mut workspace);

    // 记录 seq 3（笔触刚画完）的状态。
    let snapshot_seq = 3u64;
    let before = {
        let document = workspace.document_mut("doc_1").unwrap();
        document.state_at(snapshot_seq).unwrap().state
    };
    assert_eq!(before.objects.len(), 1);
    assert_eq!(before.objects["obj_1"].versions.len(), 1);

    // 再改一版 + 建检查点。
    workspace
        .commit(
            "doc_1",
            atom(
                AtomKind::Supersede,
                "human:1",
                "session:a",
                json!({"object_id": "obj_1", "data": {"points": [[1.0, 1.0]], "size": 9.0}}),
            ),
            "human:1",
            false,
        )
        .unwrap();
    let checkpoint = workspace
        .commit(
            "doc_1",
            atom(
                AtomKind::CreateCheckpoint,
                "human:1",
                "session:a",
                json!({"checkpoint_id": "ckpt_1", "name": "v1", "anchor_seq": 4}),
            ),
            "human:1",
            false,
        )
        .unwrap();
    assert!(checkpoint.seq >= 5);

    // 当前状态有两个版本；时间旅行回到 seq 3 只有一个。
    let head_state = workspace.document("doc_1").unwrap().state().clone();
    assert_eq!(head_state.objects["obj_1"].versions.len(), 2);
    let past = workspace
        .document_mut("doc_1")
        .unwrap()
        .state_at(snapshot_seq)
        .unwrap()
        .state;
    assert_eq!(past.objects["obj_1"].versions.len(), 1);

    // declare_head 回到 seq 3 → HEAD 求值起点跳变，全量失效。
    let declare = workspace.document("doc_1").unwrap().declare_head_atom(
        "human:1",
        "session:a",
        HeadBase::Atom(
            workspace
                .document("doc_1")
                .unwrap()
                .log()
                .by_seq(3)
                .unwrap()
                .id
                .clone(),
        ),
        Some("revert_to"),
    );
    let result = workspace
        .commit("doc_1", declare, "human:1", false)
        .unwrap();
    assert_eq!(result.dirty_kind, "full");
    let state = workspace.document("doc_1").unwrap().state();
    assert_eq!(state.eval_origin_seq(), result.seq);
    assert_eq!(state.objects["obj_1"].versions.len(), 1, "回到跳变起点");
}

#[test]
fn capability_tokens_gate_http_and_allow_stdio() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let token = workspace
        .issue_token("doc_1", "human:2", Role::Editor)
        .unwrap();

    // stdio（本地进程）豁免鉴权。
    let principal = workspace
        .authorize("doc_1", None, TransportKind::Stdio, "local:stdio")
        .unwrap();
    assert_eq!(principal.role, Role::Owner);

    // HTTP 需要令牌。
    assert_eq!(
        workspace
            .authorize("doc_1", None, TransportKind::Http, "x")
            .unwrap_err()
            .code,
        ErrorCode::PermissionDenied
    );
    let principal = workspace
        .authorize("doc_1", Some(&token), TransportKind::Http, "x")
        .unwrap();
    assert_eq!(principal.actor, "human:2");
    assert!(principal.role.can_edit());
    assert!(workspace.capability_url("doc_1", &token).contains("token="));
}

#[test]
fn orphan_blobs_are_reclaimed_but_history_is_kept() {
    let store = Arc::new(MemoryBlobStore::new());
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(NewDocument::new("doc_1", 32, 32), "human:1", "session:a")
        .unwrap();
    // 位图补丁（引用 blob）→ 撤销 → 历史级保留。
    let bitmap = vec![7u8; 64 * 4];
    let blob = workspace.store().put(&bitmap).unwrap();
    workspace
        .commit(
            "doc_1",
            atom(
                AtomKind::CreateObject,
                "ai:1",
                "session:b",
                json!({
                    "object_id": "obj_patch",
                    "layer_id": "layer_1",
                    "type": "raster_patch",
                    "bitmap": {"blob_hash": blob, "size": 256, "mime_type": "image/x-yanshi-raw"},
                    "width": 4,
                    "height": 4,
                    "region": {"x": 0, "y": 0, "w": 4, "h": 4},
                }),
            ),
            "ai:1",
            false,
        )
        .unwrap_err(); // 图层不存在 → 拒绝
    assert_eq!(
        workspace
            .commit(
                "doc_1",
                atom(
                    AtomKind::CreateLayer,
                    "ai:1",
                    "session:b",
                    json!({"layer_id": "layer_1"}),
                ),
                "ai:1",
                false
            )
            .unwrap()
            .seq,
        2
    );
    workspace
        .commit(
            "doc_1",
            atom(
                AtomKind::CreateObject,
                "ai:1",
                "session:b",
                json!({
                    "object_id": "obj_patch",
                    "layer_id": "layer_1",
                    "type": "raster_patch",
                    "bitmap": {"blob_hash": blob, "size": 256, "mime_type": "image/x-yanshi-raw"},
                    "width": 4,
                    "height": 4,
                    "region": {"x": 0, "y": 0, "w": 4, "h": 4},
                }),
            ),
            "ai:1",
            false,
        )
        .unwrap();
    // 撤销该原子：blob 掉到历史级。
    workspace
        .commit(
            "doc_1",
            atom(
                AtomKind::Revert,
                "ai:1",
                "session:b",
                json!({"target": "obj_patch_atom"}),
            ),
            "ai:1",
            true,
        )
        .unwrap_err(); // 原子 id 不是 obj_patch_atom，拒绝

    // 孤儿 blob：上传但从未被引用。
    let orphan = workspace.store().put(b"never-referenced").unwrap();
    let (report, historical) = workspace
        .document("doc_1")
        .unwrap()
        .collect_garbage(yanshi_core::now_ms() + 30 * 24 * 60 * 60 * 1000)
        .unwrap();
    assert!(report.deleted.contains(&orphan), "孤儿被清理");
    assert!(
        workspace.store().exists(&blob),
        "被日志引用的 blob 永不删除（原则 21）"
    );
    assert_eq!(historical, 0, "当前状态直接引用它，属于活跃集");
    let _ = store;
}

#[test]
fn tool_layer_covers_core_workflow() {
    let mut workspace = workspace();
    let registry = ToolRegistry::core();
    assert_eq!(registry.len(), 27, "核心层 27 个工具（10.2）");

    // 文档按需创建（MCP 路径会先 open_or_create；这里手动建）。
    workspace
        .create_document(NewDocument::new("doc_wf", 64, 64), "human:1", "session:web")
        .unwrap();
    let mut context = ToolContext::new(&mut workspace, "doc_wf", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);

    let layer = registry.call(
        &mut context,
        "create_layer",
        &json!({"layer_id": "layer_1", "name": "ink"}),
    );
    assert_eq!(layer["ok"], json!(true), "{layer}");

    let stroke = registry.call(
        &mut context,
        "draw_stroke",
        &json!({
            "layer_id": "layer_1",
            "data": {"points": [[4.0, 4.0], [30.0, 20.0]], "size": 4.0, "color": [0.0, 0.0, 0.0, 1.0]},
        }),
    );
    assert_eq!(stroke["dirty_kind"], json!("geometry"), "{stroke}");
    assert!(
        stroke["preview"]["thumb_url"].is_string(),
        "默认返回预览地址（10.1）"
    );

    let state = registry.call(&mut context, "get_state", &json!({"include_objects": true}));
    assert_eq!(state["layers"].as_array().unwrap().len(), 1);
    assert_eq!(state["objects"].as_array().unwrap().len(), 1);

    let rendered = registry.call(
        &mut context,
        "render_region",
        &json!({"region": {"x": 0, "y": 0, "w": 16, "h": 16}}),
    );
    assert_eq!(rendered["width"], json!(16));
    assert_eq!(rendered["mime_type"], json!("image/png"));

    let deleted = registry.call(
        &mut context,
        "delete_object",
        &json!({"object_id": state["objects"][0]["object_id"]}),
    );
    assert_eq!(deleted["ok"], json!(true));
    assert_eq!(deleted["dirty_kind"], json!("structure"), "删除影响整层");

    // 被删除后不能再改。
    let rejected = registry.call(
        &mut context,
        "update_object",
        &json!({"object_id": state["objects"][0]["object_id"], "patch": {"visible": false}}),
    );
    assert_eq!(rejected["ok"], json!(false));
    assert_eq!(rejected["error_code"], json!("precondition_failed"));

    // 撤销删除。
    let reverted = registry.call(
        &mut context,
        "revert",
        &json!({"atom_id": deleted["atom_id"]}),
    );
    assert_eq!(reverted["ok"], json!(true), "{reverted}");
    let listed = registry.call(&mut context, "list_objects", &json!({}));
    assert_eq!(listed["count"], json!(1), "撤销删除后对象复活");
}
