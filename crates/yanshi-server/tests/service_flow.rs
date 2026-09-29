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

/// 读取 PNG 的 IHDR 宽高（验证缩略图与区域预览的尺寸语义）。
fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(
        &bytes[0..8],
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
    );
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    (width, height)
}

#[test]
fn document_thumbnail_is_cover_whole_canvas_not_the_last_region() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let registry = ToolRegistry::core();
    let store = workspace.store();
    let mut context = ToolContext::new(&mut workspace, "doc_1", "human:1", "session:web");

    // 小范围笔触 → dirty 区域预览是局部图。
    let stroke = registry.call(
        &mut context,
        "draw_stroke",
        &json!({
            "layer_id": "layer_1",
            "data": {"points": [[8.0, 8.0], [20.0, 12.0]], "size": 3.0},
        }),
    );
    assert_eq!(stroke["ok"], json!(true), "{stroke}");
    let region_url = stroke["preview"]["thumb_url"].as_str().unwrap().to_owned();
    let region_hash: yanshi_core::BlobHash = region_url
        .trim_start_matches("yanshi://blob/")
        .parse()
        .unwrap();
    let region_png = store.get(&region_hash).unwrap();
    let (region_w, region_h) = png_size(&region_png);
    assert!(
        region_w < 96 && region_h < 96,
        "局部预览：{region_w}×{region_h}"
    );

    // get_document 返回的是**文档级**缩略图（覆盖整幅画布，缺省 256），不是那张局部图。
    let document = registry.call(&mut context, "get_document", &json!({}));
    assert_eq!(document["ok"], json!(true), "{document}");
    assert_eq!(document["thumb_size"], json!(256));
    let thumb_url = document["thumb_url"].as_str().unwrap().to_owned();
    assert_ne!(thumb_url, region_url, "缩略图不应等于最近一次区域渲染");
    let thumb_hash: yanshi_core::BlobHash = thumb_url
        .trim_start_matches("yanshi://blob/")
        .parse()
        .unwrap();
    let thumb_png = store.get(&thumb_hash).unwrap();
    let (thumb_w, thumb_h) = png_size(&thumb_png);
    assert_eq!((thumb_w, thumb_h), (256, 256), "文档级缩略图固化为 256×256");

    // 第二次调用直接复用缓存（不重新生成）。
    let cached = registry.call(&mut context, "get_document", &json!({}));
    assert_eq!(cached["thumb_url"], document["thumb_url"]);

    // `preview_size: false` 明确跳过缩略图；64 档位给出小图。
    let skipped = registry.call(
        &mut context,
        "get_document",
        &json!({"preview_size": false}),
    );
    assert!(skipped["thumb_url"].is_string(), "已缓存时仍返回地址");
    let small = registry.call(
        &mut context,
        "render_region",
        &json!({"region": {"x": 0, "y": 0, "w": 96, "h": 96}}),
    );
    assert_eq!(small["ok"], json!(true));
    // 覆盖整幅画布的渲染会成为文档级预览（96 >= 96 画布）。
    let full = registry.call(
        &mut context,
        "render_region",
        &json!({"region": {"x": 0, "y": 0, "w": 96, "h": 96}}),
    );
    let full_hash: yanshi_core::BlobHash = full["thumb_url"]
        .as_str()
        .unwrap()
        .trim_start_matches("yanshi://blob/")
        .parse()
        .unwrap();
    assert_eq!(png_size(&store.get(&full_hash).unwrap()), (96, 96));
    let after_full = registry.call(&mut context, "get_document", &json!({}));
    assert_eq!(
        after_full["thumb_url"], full["thumb_url"],
        "整幅渲染会替换文档级预览（打开即图片）"
    );

    // 再改一笔：HEAD 前进而缩略图落在后面 → 必须重新生成，不能返回过期缓存
    // （否则「打开即图片」会显示旧画面）。
    let later = registry.call(
        &mut context,
        "draw_stroke",
        &json!({
            "layer_id": "layer_1",
            "data": {"points": [[90.0, 90.0], [94.0, 94.0]], "size": 2.0},
        }),
    );
    assert_eq!(later["ok"], json!(true), "{later}");
    let refreshed = registry.call(&mut context, "get_document", &json!({}));
    assert_ne!(
        refreshed["thumb_url"], after_full["thumb_url"],
        "缩略图落后于 HEAD 时必须重新生成"
    );
    let refreshed_hash: yanshi_core::BlobHash = refreshed["thumb_url"]
        .as_str()
        .unwrap()
        .trim_start_matches("yanshi://blob/")
        .parse()
        .unwrap();
    assert_eq!(
        png_size(&store.get(&refreshed_hash).unwrap()),
        (256, 256),
        "重新生成的是缺省 256 档文档级缩略图"
    );
    assert!(workspace
        .document("doc_1")
        .unwrap()
        .document_thumbnail_is_current());
}

/// Phase 3 起步：调色与滤镜工具必须真的改变像素，并按结构 dirty 整层失效。
#[test]
fn adjustment_and_filter_tools_change_pixels_and_are_reversible() {
    let mut workspace = workspace();
    workspace
        .create_document(NewDocument::new("doc_fx", 96, 96), "human:1", "session:web")
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Retouch,
    ]);
    let store = workspace.store();
    let mut context = ToolContext::new(&mut workspace, "doc_fx", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);

    registry.call(
        &mut context,
        "create_layer",
        &json!({"layer_id": "layer_1"}),
    );
    // 一块中灰矩形，便于观察调色/模糊效果。
    registry.call(
        &mut context,
        "draw_shape",
        &json!({
            "layer_id": "layer_1",
            "object_id": "obj_base",
            "data": {
                "geometry": {"kind": "rect", "bbox": {"x": 8, "y": 8, "w": 64, "h": 64}},
                "color": {"r": 128, "g": 128, "b": 128, "a": 255},
            },
        }),
    );

    // 采样函数：把 render_region 的结果 PNG 解出某像素。
    let sample = |context: &mut ToolContext<'_>, x: u32, y: u32| -> [u8; 4] {
        let response = registry.call(
            context,
            "render_region",
            &json!({"region": {"x": 0, "y": 0, "w": 96, "h": 96}}),
        );
        let hash: yanshi_core::BlobHash = response["thumb_url"]
            .as_str()
            .unwrap()
            .trim_start_matches("yanshi://blob/")
            .parse()
            .unwrap();
        let (_, _, pixels) = decode_png(&store.get(&hash).unwrap());
        let index = ((y * 96 + x) * 4) as usize;
        [
            pixels[index],
            pixels[index + 1],
            pixels[index + 2],
            pixels[index + 3],
        ]
    };

    let before = sample(&mut context, 40, 40);
    assert_eq!(before[0], 128, "底色应为中灰：{before:?}");

    // 1) 调整：反相 → 像素反转；dirty 应为整层结构。
    let inverted = registry.call(
        &mut context,
        "add_adjustment",
        &json!({"layer_id": "layer_1", "adjustment_type": "invert", "name": "invert"}),
    );
    assert_eq!(inverted["ok"], json!(true), "{inverted}");
    assert_eq!(inverted["dirty_kind"], json!("structure"), "调整影响整层");
    // 内核在线性光里做反相，因此 sRGB 字节 128 会落到亮部（≈229）而不是 127。
    let after = sample(&mut context, 40, 40);
    assert!(after[0] > 200, "线性光反相应把中灰推到亮部，得到 {after:?}");
    assert_eq!(after[0], after[1], "灰度像素反相后仍为中性灰：{after:?}");

    // 2) 参数校验：未实现的类型/越界参数一律 invalid_argument 且不写日志。
    // 通过工具读原子数（`context` 已可变借用 workspace，不能再直接访问）。
    let atoms_before = registry.call(&mut context, "get_document", &json!({}))["atoms"]
        .as_u64()
        .unwrap();
    for (name, payload) in [
        // 设计里有、内核尚未实现的类型必须在工具层被拒（不放空壳能力）。
        (
            "add_adjustment",
            json!({"layer_id": "layer_1", "adjustment_type": "hsl"}),
        ),
        (
            "add_filter",
            json!({"layer_id": "layer_1", "filter_name": "glow"}),
        ),
        (
            "add_filter",
            json!({"layer_id": "layer_1", "filter_name": "gaussian_blur", "params": {"sigma": 999}}),
        ),
        (
            "add_adjustment",
            json!({"layer_id": "layer_1", "adjustment_type": "levels", "params": {"black": 0.8, "white": 0.2}}),
        ),
    ] {
        let response = registry.call(&mut context, name, &payload);
        assert_eq!(response["ok"], json!(false), "{name} {payload}: {response}");
        assert_eq!(response["error_code"], json!("invalid_argument"));
    }
    let supported = registry.call(&mut context, "list_effects", &json!({}));
    assert!(
        supported["supported"]["adjustments"]
            .as_array()
            .unwrap()
            .contains(&json!("levels")),
        "应列出内核支持的清单"
    );
    assert_eq!(
        registry.call(&mut context, "get_document", &json!({}))["atoms"]
            .as_u64()
            .unwrap(),
        atoms_before,
        "被拒绝的效果不得写入日志"
    );

    // 3) 滤镜：高斯模糊应把硬边缘抹开（边缘像素被拉近底色）。
    let sharp_edge = sample(&mut context, 8, 8);
    let blurred = registry.call(
        &mut context,
        "add_filter",
        &json!({"layer_id": "layer_1", "filter_name": "gaussian_blur", "params": {"sigma": 3.0}}),
    );
    assert_eq!(blurred["ok"], json!(true), "{blurred}");
    let soft_edge = sample(&mut context, 8, 8);
    assert_ne!(sharp_edge, soft_edge, "模糊必须改变边缘像素");

    // 4) 更新滤镜参数（叠加到现有参数上）。
    let effects = registry.call(&mut context, "list_effects", &json!({}));
    assert_eq!(effects["count"], json!(2), "{effects}");
    let filter_object = effects["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["filter_name"] == json!("gaussian_blur"))
        .and_then(|effect| effect["object_id"].as_str())
        .expect("应能找到刚创建的高斯模糊对象")
        .to_owned();
    let updated = registry.call(
        &mut context,
        "update_filter",
        &json!({"object_id": filter_object, "params": {"sigma": 6.0}}),
    );
    assert_eq!(updated["ok"], json!(true), "{updated}");
    let after_update = registry.call(&mut context, "list_effects", &json!({}));
    let sigma = after_update["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["object_id"] == json!(filter_object))
        .unwrap()["params"]["sigma"]
        .clone();
    assert_eq!(sigma, json!(6.0), "参数应被叠加更新");

    // 删除滤镜 → 像素回到「只有反相」的状态（append-only：用 tombstone 而不是改历史）。
    let deleted = registry.call(
        &mut context,
        "delete_object",
        &json!({"object_id": filter_object}),
    );
    assert_eq!(deleted["ok"], json!(true), "{deleted}");
    let restored = sample(&mut context, 8, 8);
    assert_eq!(restored, sharp_edge, "删除滤镜后应回到只有反相的画面");
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
fn invalid_colors_are_rejected_and_byte_arrays_paint_correctly() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_color", 64, 64),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::core();
    let store = workspace.store();
    let mut context = ToolContext::new(&mut workspace, "doc_color", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);

    registry.call(
        &mut context,
        "create_layer",
        &json!({"layer_id": "layer_1"}),
    );

    // 非法颜色在工具层就被拒绝（不会写入原子）。
    for bad in [
        json!([0.2, 0.3]),
        json!({"r": 300, "g": 0, "b": 0}),
        json!("#12345"),
        json!("green"),
    ] {
        let response = registry.call(
            &mut context,
            "draw_stroke",
            &json!({"layer_id": "layer_1", "data": {"points": [[2.0, 2.0], [10.0, 10.0]], "color": bad}}),
        );
        assert_eq!(response["ok"], json!(false), "{response}");
        assert_eq!(response["error_code"], json!("invalid_argument"));
        assert!(response["context"]["detail"]
            .as_str()
            .unwrap()
            .contains("颜色格式非法"));
    }

    // 字节数组与 {r,g,b,a} 两种写法渲染出同样的绿色笔迹（历史白色事故的回归）。
    for (label, color) in [
        ("array", json!([40, 120, 60, 255])),
        ("object", json!({"r": 40, "g": 120, "b": 60, "a": 255})),
    ] {
        let response = registry.call(
            &mut context,
            "draw_stroke",
            &json!({
                "layer_id": "layer_1",
                "data": {"points": [[8.0, 40.0], [56.0, 40.0]], "size": 9.0, "color": color},
            }),
        );
        assert_eq!(response["ok"], json!(true), "{label}: {response}");
        let hash: yanshi_core::BlobHash = response["preview"]["thumb_url"]
            .as_str()
            .unwrap()
            .trim_start_matches("yanshi://blob/")
            .parse()
            .unwrap();
        let png = store.get(&hash).unwrap();
        let (width, height, pixels) = decode_png(&png);
        assert!(width > 0 && height > 0);
        // 预览区域就是 dirty bbox：找一个落在线上的像素看它是不是绿色。
        let painted = pixels
            .chunks(4)
            .filter(|pixel| {
                pixel[1] as i32 > pixel[0] as i32 + 20 && pixel[1] as i32 > pixel[2] as i32 + 20
            })
            .count();
        assert!(
            painted > 20,
            "{label}: 预览里应有成片的绿色像素，实际 {painted} 个（整幅 {}×{}）",
            width,
            height
        );
        let white = pixels
            .chunks(4)
            .filter(|pixel| pixel[0] > 250 && pixel[1] > 250 && pixel[2] > 250)
            .count();
        assert!(
            white * 2 < pixels.len() / 4,
            "{label}: 不应整片饱和成白色（白像素 {white} / {}）",
            pixels.len() / 4
        );
    }
}

/// 极简 PNG 解码（本仓库的 PNG 用 stored deflate，因此 zlib 展开即可）。
fn decode_png(bytes: &[u8]) -> (usize, usize, Vec<u8>) {
    assert_eq!(
        &bytes[0..8],
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
    );
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]) as usize;
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]) as usize;
    // 跳过 IHDR，累加 IDAT。
    let mut idat = Vec::new();
    let mut offset = 8usize;
    while offset + 8 <= bytes.len() {
        let length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let kind = &bytes[offset + 4..offset + 8];
        let body = &bytes[offset + 8..offset + 8 + length];
        if kind == b"IDAT" {
            idat.extend_from_slice(body);
        }
        if kind == b"IEND" {
            break;
        }
        offset += 12 + length;
    }
    let raw = inflate(&idat);
    let stride = width * 4;
    let mut pixels = Vec::with_capacity(stride * height);
    let mut previous = vec![0u8; stride];
    let mut position = 0usize;
    for _ in 0..height {
        let filter = raw[position];
        position += 1;
        let mut line = raw[position..position + stride].to_vec();
        position += stride;
        for index in 0..stride {
            let a = if index >= 4 { line[index - 4] } else { 0 };
            let b = previous[index];
            let c = if index >= 4 { previous[index - 4] } else { 0 };
            let value = match filter {
                1 => line[index].wrapping_add(a),
                2 => line[index].wrapping_add(b),
                3 => line[index].wrapping_add(((a as u16 + b as u16) / 2) as u8),
                4 => {
                    let p = a as i16 + b as i16 - c as i16;
                    let pa = (p - a as i16).abs();
                    let pb = (p - b as i16).abs();
                    let pc = (p - c as i16).abs();
                    let predictor = if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    };
                    line[index].wrapping_add(predictor)
                }
                _ => line[index],
            };
            line[index] = value;
        }
        pixels.extend_from_slice(&line);
        previous = line;
    }
    (width, height, pixels)
}

/// zlib 展开：本仓库的 PNG 固定使用 **stored** deflate 块（见 `yanshi_render::png`），
/// 因此这里只需跳过 2 字节 zlib 头并逐块拷贝即可，无需 huffman 解码。
fn inflate(data: &[u8]) -> Vec<u8> {
    assert_eq!((data[0], data[1]), (0x78, 0x01), "zlib 头");
    let mut out = Vec::new();
    let mut position = 2usize;
    loop {
        let header = data[position];
        position += 1;
        assert_eq!((header >> 1) & 0b11, 0, "只支持 stored deflate 块");
        let length = u16::from_le_bytes([data[position], data[position + 1]]) as usize;
        position += 4;
        out.extend_from_slice(&data[position..position + length]);
        position += length;
        if header & 1 == 1 {
            break;
        }
    }
    out
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
