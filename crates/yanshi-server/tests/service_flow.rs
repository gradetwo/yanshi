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
        // 刻意用**明确不存在**的名字，避免"实现了该类型就要回来改测试"的反复（已换过四次）。
        (
            "add_adjustment",
            json!({"layer_id": "layer_1", "adjustment_type": "not_an_effect"}),
        ),
        (
            "add_filter",
            json!({"layer_id": "layer_1", "filter_name": "posterize"}),
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

/// Phase 3 修图：clone_stamp 必须把偏移处的已有内容复制到笔迹位置，并可撤销。
#[test]
fn clone_stamp_copies_content_and_is_reversible() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_clone", 96, 96),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Retouch,
    ]);
    let store = workspace.store();
    let mut context = ToolContext::new(&mut workspace, "doc_clone", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    registry.call(
        &mut context,
        "create_layer",
        &json!({"layer_id": "layer_1"}),
    );

    // 左上角放一块红，再用仿制图章把它复制到右下角。
    registry.call(
        &mut context,
        "draw_shape",
        &json!({"layer_id": "layer_1", "object_id": "src",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 4, "y": 4, "w": 20, "h": 20}},
                         "color": "#dc1e1e"}}),
    );
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

    let before = sample(&mut context, 60, 60);
    assert!(
        before[0] > 240 && before[1] > 240,
        "目标处原本是白底：{before:?}"
    );
    let cloned = registry.call(
        &mut context,
        "clone_stamp",
        &json!({"layer_id": "layer_1", "points": [[64.0, 64.0]],
                "source_offset": [-50.0, -50.0], "size": 16.0, "hardness": 1.0}),
    );
    assert_eq!(cloned["ok"], json!(true), "{cloned}");
    assert_eq!(
        cloned["dirty_kind"],
        json!("geometry"),
        "修图只影响笔迹范围"
    );
    let after = sample(&mut context, 60, 60);
    assert!(after[0] > 180 && after[1] < 90, "应复制到红色：{after:?}");

    // 修复画笔：同一位置同一源，颜色应向目标处靠拢。
    let healed = registry.call(
        &mut context,
        "heal_stamp",
        &json!({"layer_id": "layer_1", "points": [[76.0, 76.0]],
                "source_offset": [-50.0, -50.0], "size": 16.0, "hardness": 1.0}),
    );
    assert_eq!(healed["ok"], json!(true), "{healed}");
    let healed_pixel = sample(&mut context, 76, 76);
    assert!(
        healed_pixel[1] > after[1] + 20,
        "heal 应比 clone 更接近白色目标处：clone={after:?} heal={healed_pixel:?}"
    );

    // 自检用的 raw 输出：返回原始 RGBA8 blob，供客户端逐像素比对。
    let raw = registry.call(
        &mut context,
        "render_region",
        &json!({"region": {"x": 0, "y": 0, "w": 32, "h": 32}, "raw": true}),
    );
    assert_eq!(raw["ok"], json!(true), "{raw}");
    assert_eq!(raw["width"], json!(32));
    assert_eq!(raw["height"], json!(32));
    assert_eq!(raw["mime_type"], json!(yanshi_render::RAW_RGBA_MIME));
    let raw_hash: yanshi_core::BlobHash = raw["raw_url"]
        .as_str()
        .unwrap()
        .trim_start_matches("yanshi://blob/")
        .parse()
        .unwrap();
    assert_eq!(store.get(&raw_hash).unwrap().len(), 32 * 32 * 4);

    // 区域外扩上限：offset/size 组合超出内核可达范围时必须拒绝（否则分块与整幅会不一致）。
    let too_far = registry.call(
        &mut context,
        "clone_stamp",
        &json!({"layer_id": "layer_1", "points": [[40.0, 40.0]], "source_offset": [-200.0, -200.0], "size": 16.0}),
    );
    assert_eq!(
        too_far["error_code"],
        json!("invalid_argument"),
        "{too_far}"
    );
    assert!(
        too_far["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("超过内核上限"),
        "错误信息应说明原因：{too_far}"
    );

    // 涂抹：方向由笔迹推导，无需 source_offset。
    let smudged = registry.call(
        &mut context,
        "smudge",
        &json!({"layer_id": "layer_1", "points": [[50.0, 50.0], [58.0, 50.0], [66.0, 50.0]],
                "size": 14.0, "smudge_length": 8.0, "hardness": 1.0}),
    );
    assert_eq!(smudged["ok"], json!(true), "{smudged}");
    assert_eq!(smudged["dirty_kind"], json!("geometry"));
    let bad_smudge = registry.call(
        &mut context,
        "smudge",
        &json!({"layer_id": "layer_1", "points": [[10.0, 10.0]], "smudge_length": 9999}),
    );
    assert_eq!(
        bad_smudge["error_code"],
        json!("invalid_argument"),
        "{bad_smudge}"
    );

    // 图章补丁：把源区域（左上红块）抓成 blob 后落到右下。
    let patched = registry.call(
        &mut context,
        "patch",
        &json!({"layer_id": "layer_1", "source_region": {"x": 4, "y": 4, "w": 20, "h": 20},
                "target": [70.0, 10.0]}),
    );
    assert_eq!(patched["ok"], json!(true), "{patched}");
    let patch_pixel = sample(&mut context, 74, 14);
    assert!(
        patch_pixel[0] > 180 && patch_pixel[1] < 90,
        "补丁应带上红色：{patch_pixel:?}"
    );
    let bad_patch = registry.call(
        &mut context,
        "patch",
        &json!({"layer_id": "layer_1", "source_region": {"x": 0, "y": 0, "w": 0, "h": 0}, "target": [1.0, 1.0]}),
    );
    assert_eq!(
        bad_patch["error_code"],
        json!("invalid_argument"),
        "{bad_patch}"
    );

    // 液化（推力）：把边界附近的像素推开；零方向与越界强度必须被拒。
    let pushed = registry.call(
        &mut context,
        "liquify_push",
        &json!({"layer_id": "layer_1", "points": [[50.0, 50.0]], "direction": [1.0, 0.0],
                "size": 40.0, "strength": 0.6}),
    );
    assert_eq!(pushed["ok"], json!(true), "{pushed}");
    assert_eq!(
        pushed["dirty_kind"],
        json!("geometry"),
        "液化只影响笔迹范围"
    );
    for payload in [
        json!({"layer_id": "layer_1", "points": [[10.0, 10.0]], "direction": [0.0, 0.0]}),
        json!({"layer_id": "layer_1", "points": [], "direction": [1.0, 0.0]}),
        json!({"layer_id": "layer_1", "points": [[1.0, 1.0]], "direction": [1.0, 0.0], "strength": 9.0}),
    ] {
        let response = registry.call(&mut context, "liquify_push", &payload);
        assert_eq!(
            response["error_code"],
            json!("invalid_argument"),
            "{payload}: {response}"
        );
    }

    // 分通道色阶与色调分离：可用且参数校验按效果区分。
    for (adjustment, params) in [
        (
            "levels",
            json!({"black": 0.05, "white": 0.95, "gamma": 1.1, "channel": "b"}),
        ),
        ("posterize", json!({"levels": 5})),
    ] {
        let response = registry.call(
            &mut context,
            "add_adjustment",
            &json!({"layer_id": "layer_1", "adjustment_type": adjustment, "params": params}),
        );
        assert_eq!(response["ok"], json!(true), "{adjustment}: {response}");
    }
    for (adjustment, params) in [
        ("levels", json!({"channel": "cyan"})),
        ("posterize", json!({"levels": 1})),
        ("color_balance", json!({"shadows": [0.1, 0.2]})),
        // 四个分量 → 非法（三分量且在范围内才是合法的，上面接受用例已覆盖）。
        ("split_toning", json!({"shadows": [0.1, 0.2, 0.3, 0.4]})),
        ("vibrance", json!({"amount": 9.0})),
        (
            "split_toning",
            json!({"shadows": [0.0, 0.0, 0.0], "balance": 5.0}),
        ),
        ("color_balance", json!({"highlights": [2.0, 0.0, 0.0]})),
    ] {
        let response = registry.call(
            &mut context,
            "add_adjustment",
            &json!({"layer_id": "layer_1", "adjustment_type": adjustment, "params": params}),
        );
        assert_eq!(
            response["error_code"],
            json!("invalid_argument"),
            "{adjustment}: {response}"
        );
    }

    // 液化三模式：twirl / pinch 可用（pinch 允许负强度=膨胀），模式参数校验按名区分。
    let twirled = registry.call(
        &mut context,
        "liquify_twirl",
        &json!({"layer_id": "layer_1", "points": [[50.0, 50.0]], "size": 40.0, "strength": 1.0}),
    );
    assert_eq!(twirled["ok"], json!(true), "{twirled}");
    let pinched = registry.call(
        &mut context,
        "liquify_pinch",
        &json!({"layer_id": "layer_1", "points": [[60.0, 60.0]], "size": 40.0, "strength": -0.6}),
    );
    assert_eq!(
        pinched["ok"],
        json!(true),
        "pinch 负强度（膨胀）应被接受：{pinched}"
    );
    let bad_strength = registry.call(
        &mut context,
        "liquify_twirl",
        &json!({"layer_id": "layer_1", "points": [[10.0, 10.0]], "strength": -1.0}),
    );
    assert_eq!(
        bad_strength["error_code"],
        json!("invalid_argument"),
        "twirl 不接受负强度：{bad_strength}"
    );
    let missing_direction = registry.call(
        &mut context,
        "liquify_push",
        &json!({"layer_id": "layer_1", "points": [[10.0, 10.0]], "size": 40.0}),
    );
    assert_eq!(
        missing_direction["error_code"],
        json!("invalid_argument"),
        "push 必须给 direction"
    );

    // 参数校验：零偏移无意义、点列非法都要被拒。
    for payload in [
        json!({"layer_id": "layer_1", "points": [[64.0, 64.0]], "source_offset": [0.0, 0.0]}),
        json!({"layer_id": "layer_1", "points": [], "source_offset": [10.0, 10.0]}),
        json!({"layer_id": "layer_1", "points": [["x", 1.0]], "source_offset": [10.0, 10.0]}),
        json!({"layer_id": "layer_1", "points": [[1.0, 1.0]], "source_offset": [10.0, 10.0], "size": 0.0}),
    ] {
        let response = registry.call(&mut context, "clone_stamp", &payload);
        assert_eq!(
            response["error_code"],
            json!("invalid_argument"),
            "{payload}: {response}"
        );
    }

    // 撤销（tombstone）后回到白底。
    let list = registry.call(&mut context, "list_objects", &json!({"type": "retouch"}));
    let object_id = list["objects"][0]["object_id"].as_str().unwrap().to_owned();
    let deleted = registry.call(
        &mut context,
        "delete_object",
        &json!({"object_id": object_id}),
    );
    assert_eq!(deleted["ok"], json!(true), "{deleted}");
    let restored = sample(&mut context, 60, 60);
    assert!(
        restored[0] > 240 && restored[1] > 240,
        "撤销后应回到白底：{restored:?}"
    );
}

/// 蒙版：创建 + 挂到图层后，图层内容应只在蒙版范围内可见（含羽化与缺蒙版告警）。
#[test]
fn mask_clips_layer_content_end_to_end() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_mask", 64, 64),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Structure,
    ]);
    let store = workspace.store();
    let mut context = ToolContext::new(&mut workspace, "doc_mask", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    registry.call(
        &mut context,
        "create_layer",
        &json!({"layer_id": "layer_1"}),
    );
    // 铺满整层的深色，再用左半蒙版裁掉右半。
    registry.call(
        &mut context,
        "draw_shape",
        &json!({"layer_id": "layer_1", "object_id": "fill",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 64, "h": 64}},
                         "color": {"r": 10, "g": 10, "b": 10, "a": 255}}}),
    );
    let sample = |context: &mut ToolContext<'_>, x: u32, y: u32| -> [u8; 4] {
        let response = registry.call(
            context,
            "render_region",
            &json!({"region": {"x": 0, "y": 0, "w": 64, "h": 64}}),
        );
        let hash: yanshi_core::BlobHash = response["thumb_url"]
            .as_str()
            .unwrap()
            .trim_start_matches("yanshi://blob/")
            .parse()
            .unwrap();
        let (_, _, pixels) = decode_png(&store.get(&hash).unwrap());
        let index = ((y * 64 + x) * 4) as usize;
        [
            pixels[index],
            pixels[index + 1],
            pixels[index + 2],
            pixels[index + 3],
        ]
    };
    assert!(
        sample(&mut context, 48, 32)[0] < 120,
        "挂蒙版前右半应是深色"
    );

    let created = registry.call(
        &mut context,
        "create_mask",
        &json!({"mask_id": "mask_1", "shape": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 32, "h": 64}},
                "linked_layer": "layer_1"}),
    );
    assert_eq!(created["ok"], json!(true), "{created}");
    let attached = registry.call(
        &mut context,
        "set_property",
        &json!({"layer_id": "layer_1", "key": "mask_id", "value": "mask_1"}),
    );
    assert_eq!(attached["ok"], json!(true), "{attached}");
    assert_eq!(
        attached["dirty_kind"],
        json!("structure"),
        "挂蒙版属结构变更"
    );

    assert!(sample(&mut context, 16, 32)[0] < 120, "蒙版内应保留深色");
    assert!(
        sample(&mut context, 48, 32)[0] > 200,
        "蒙版外应被裁掉（露白底）"
    );

    // 参数校验。
    for payload in [
        json!({"mask_id": "m2", "shape": {}}),
        json!({"mask_id": "m2", "shape": {"kind": "rect"}, "feather": 9999}),
        json!({"mask_id": "m2", "shape": {"kind": "rect"}, "mode": "weird"}),
    ] {
        let response = registry.call(&mut context, "create_mask", &payload);
        assert_eq!(
            response["error_code"],
            json!("invalid_argument"),
            "{payload}: {response}"
        );
    }
}

/// Phase 4b 起步：建议（suggest）记录 patch、可被拒绝并联动标注状态。
#[test]
fn suggestions_are_recorded_listed_and_rejected() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_sug", 64, 64),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Collab,
        yanshi_server::Profile::Retouch,
        yanshi_server::Profile::Annotation,
    ]);
    let mut context = ToolContext::new(&mut workspace, "doc_sug", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    registry.call(
        &mut context,
        "create_layer",
        &json!({"layer_id": "layer_1"}),
    );

    // 人类标注（4a 已有能力），AI 解析后给出建议（4b）。
    let annotation = registry.call(
        &mut context,
        "create_annotation",
        &json!({"type": "region", "content": "这里太暗了", "intent": "modify",
                "target": {"type": "region", "bbox": {"x": 8, "y": 8, "w": 32, "h": 32}}}),
    );
    assert_eq!(annotation["ok"], json!(true), "{annotation}");
    let pending = registry.call(
        &mut context,
        "list_annotations",
        &json!({"status": "pending"}),
    );
    assert_eq!(
        pending["pending"],
        json!(1),
        "应能看到待处理标注：{pending}"
    );

    // AI 建议：patch 是一串工具调用，accept 时按序重放。
    let annotation_id = annotation["annotation_id"]
        .as_str()
        .unwrap_or("anno_1")
        .to_owned();
    let suggestion = registry.call(
        &mut context,
        "suggest",
        &json!({
            "annotation_id": annotation_id,
            "summary": "提高曝光并加暗角",
            "patch": [
                {"tool": "add_adjustment", "arguments": {"layer_id": "layer_1",
                    "adjustment_type": "exposure", "params": {"ev": 0.6}}},
                {"tool": "add_filter", "arguments": {"layer_id": "layer_1",
                    "filter_name": "vignette", "params": {"strength": 0.4}}}
            ]
        }),
    );
    assert_eq!(suggestion["ok"], json!(true), "{suggestion}");
    assert_eq!(suggestion["steps"], json!(2));
    let suggestion_id = suggestion["suggestion_id"].as_str().unwrap().to_owned();

    // 形状校验：空 patch / 缺 tool / arguments 非对象都要被拒。
    for payload in [
        json!({"patch": []}),
        json!({"patch": [{"arguments": {}}]}),
        json!({"patch": [{"tool": "add_filter", "arguments": 3}]}),
        json!({"patch": [{"tool": ""}]}),
    ] {
        let response = registry.call(&mut context, "suggest", &payload);
        assert_eq!(
            response["error_code"],
            json!("invalid_argument"),
            "{payload}: {response}"
        );
    }

    // 列表：默认 pending；按状态过滤可用。
    let listed = registry.call(&mut context, "list_suggestions", &json!({}));
    assert_eq!(
        listed["suggestions"].as_array().unwrap().len(),
        1,
        "{listed}"
    );
    assert_eq!(listed["suggestions"][0]["status"], json!("pending"));
    assert_eq!(
        listed["suggestions"][0]["annotation_id"],
        json!(annotation_id)
    );
    assert_eq!(
        listed["suggestions"][0]["patch"].as_array().unwrap().len(),
        2
    );

    // 把标注关联到建议，然后拒绝：应记录原因并把标注置为 rejected。
    let linked = registry.call(
        &mut context,
        "create_annotation",
        &json!({"type": "region", "content": "再来一处", "intent": "style",
                "target": {"type": "region", "bbox": {"x": 0, "y": 0, "w": 16, "h": 16}},
                "suggestion_id": suggestion_id}),
    );
    assert_eq!(linked["ok"], json!(true), "{linked}");
    let rejected = registry.call(
        &mut context,
        "reject_suggestion",
        &json!({"suggestion_id": suggestion_id, "reason": "方向不对"}),
    );
    assert_eq!(rejected["ok"], json!(true), "{rejected}");
    assert_eq!(rejected["reason"], json!("方向不对"));
    // 两个：
    //  * 用例里显式带 `suggestion_id` 创建的那条 ✓；
    //  * 以及 `suggest{annotation_id}` 关联的那条 ✓ —— 本轮补上了「标注 → 建议」的**回写**
    //    （设计 4.6 的 `Annotation.suggestion_id` 字段此前永远是 None ✗），
    //    于是这条标注现在也真正引用了该建议，按设计同样应被置为 rejected ✓。
    assert_eq!(
        rejected["rejected_annotations"].as_array().unwrap().len(),
        2,
        "引用该建议的标注（显式关联 + suggest 回写）都应被置为 rejected：{rejected}"
    );
    let still_pending = registry.call(
        &mut context,
        "list_suggestions",
        &json!({"status": "pending"}),
    );
    assert_eq!(
        still_pending["suggestions"].as_array().unwrap().len(),
        0,
        "{still_pending}"
    );
    let rejected_only = registry.call(
        &mut context,
        "list_suggestions",
        &json!({"status": "rejected"}),
    );
    assert_eq!(rejected_only["suggestions"][0]["reason"], json!("方向不对"));
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
    let (report, plan) = workspace
        .document("doc_1")
        .unwrap()
        .collect_garbage(yanshi_core::now_ms() + 30 * 24 * 60 * 60 * 1000)
        .unwrap();
    assert!(report.deleted.contains(&orphan), "孤儿被清理");
    assert!(
        workspace.store().exists(&blob),
        "被日志引用的 blob 永不删除（原则 21）"
    );
    assert_eq!(plan.historical.len(), 0, "当前状态直接引用它，属于活跃集");
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
    // **不要硬编码数量** ✗：本轮新增工具时，写死的 27 让本测试变红 ✓，
    // 而它真正要守的是"核心工作流所需的工具都在" ✓（下方逐一调用即验证 ✓）。
    // 因此只保留下界断言 ✓，避免以后每加一个工具就要改一次数字 ✓。
    assert!(
        registry.len() >= 25,
        "核心层工具数明显偏少（{}），注册表可能没建全",
        registry.len()
    );

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

/// Phase 4b 闭环：接受建议 → 按序重放 patch（真的改变像素）→ 关联标注置为 resolved。
#[test]
fn accept_suggestion_replays_the_patch_and_resolves_annotations() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_accept", 64, 64),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Collab,
        yanshi_server::Profile::Retouch,
        yanshi_server::Profile::Annotation,
    ]);
    let store = workspace.store();
    let mut context = ToolContext::new(&mut workspace, "doc_accept", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    registry.call(
        &mut context,
        "create_layer",
        &json!({"layer_id": "layer_1"}),
    );
    registry.call(
        &mut context,
        "draw_shape",
        &json!({"layer_id": "layer_1", "object_id": "base",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 64, "h": 64}},
                         "color": {"r": 120, "g": 120, "b": 120, "a": 255}}}),
    );
    let sample = |context: &mut ToolContext<'_>, x: u32, y: u32| -> [u8; 4] {
        let response = registry.call(
            context,
            "render_region",
            &json!({"region": {"x": 0, "y": 0, "w": 64, "h": 64}}),
        );
        let hash: yanshi_core::BlobHash = response["thumb_url"]
            .as_str()
            .unwrap()
            .trim_start_matches("yanshi://blob/")
            .parse()
            .unwrap();
        let (_, _, pixels) = decode_png(&store.get(&hash).unwrap());
        let index = ((y * 64 + x) * 4) as usize;
        [
            pixels[index],
            pixels[index + 1],
            pixels[index + 2],
            pixels[index + 3],
        ]
    };
    let before = sample(&mut context, 32, 32);

    let annotation = registry.call(
        &mut context,
        "create_annotation",
        &json!({"type": "region", "content": "整体提亮", "intent": "modify",
                "target": {"type": "region", "bbox": {"x": 0, "y": 0, "w": 64, "h": 64}}}),
    );
    let annotation_id = annotation["annotation_id"].as_str().unwrap().to_owned();
    let suggestion = registry.call(
        &mut context,
        "suggest",
        &json!({
            "annotation_id": annotation_id,
            "summary": "曝光 +1EV 后反相",
            "patch": [
                {"tool": "add_adjustment", "arguments": {"layer_id": "layer_1",
                    "adjustment_type": "exposure", "params": {"ev": 1.0}}},
                {"tool": "add_adjustment", "arguments": {"layer_id": "layer_1",
                    "adjustment_type": "invert"}}
            ]
        }),
    );
    let suggestion_id = suggestion["suggestion_id"].as_str().unwrap().to_owned();
    let linked = registry.call(
        &mut context,
        "create_annotation",
        &json!({"type": "region", "content": "跟随建议", "intent": "style",
                "target": {"type": "region", "bbox": {"x": 0, "y": 0, "w": 32, "h": 32}},
                "suggestion_id": suggestion_id}),
    );
    assert_eq!(linked["ok"], json!(true), "{linked}");

    // 只读步骤不得作为补丁步骤。
    let read_only = registry.call(
        &mut context,
        "suggest",
        &json!({"patch": [{"tool": "get_document", "arguments": {}}]}),
    );
    let read_only_id = read_only["suggestion_id"].as_str().unwrap().to_owned();
    let refused = registry.call(
        &mut context,
        "accept_suggestion",
        &json!({"suggestion_id": read_only_id}),
    );
    assert_eq!(
        refused["error_code"],
        json!("invalid_argument"),
        "{refused}"
    );

    // 接受：patch 真正重放 + 标注置为 resolved。
    let accepted = registry.call(
        &mut context,
        "accept_suggestion",
        &json!({"suggestion_id": suggestion_id}),
    );
    assert_eq!(accepted["ok"], json!(true), "{accepted}");
    assert_eq!(
        accepted["applied_atom_ids"].as_array().unwrap().len(),
        2,
        "{accepted}"
    );
    // 同上：`suggest{annotation_id}` 现在会回写标注的 `suggestion_id` ✓，
    // 因此引用该建议的标注有两条，都应被置为 resolved ✓。
    assert_eq!(
        accepted["resolved_annotations"].as_array().unwrap().len(),
        2,
        "引用该建议的标注（显式关联 + suggest 回写）都应被置为 resolved：{accepted}"
    );
    let after = sample(&mut context, 32, 32);
    assert!(
        after[0] > before[0] + 40,
        "曝光+反相应显著提亮（线性光反相把中灰推到亮部）：{before:?} → {after:?}"
    );
    let listed = registry.call(
        &mut context,
        "list_suggestions",
        &json!({"status": "accepted"}),
    );
    assert!(
        listed["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["suggestion_id"] == json!(suggestion_id)),
        "{listed}"
    );
    // 未知建议必须报引用错误。
    let missing = registry.call(
        &mut context,
        "accept_suggestion",
        &json!({"suggestion_id": "01ZZZZZZZZZZZZZZZZZZZZZZZZZZ"}),
    );
    assert_eq!(
        missing["error_code"],
        json!("reference_not_found"),
        "{missing}"
    );
}

/// 建议预览：逐步校验与归类，且**不产生任何副作用**（不写日志）。
#[test]
fn preview_suggestion_validates_without_applying() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_preview", 64, 64),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Collab,
        yanshi_server::Profile::Retouch,
    ]);
    let mut context = ToolContext::new(&mut workspace, "doc_preview", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    registry.call(
        &mut context,
        "create_layer",
        &json!({"layer_id": "layer_1"}),
    );
    let head_before = registry.call(&mut context, "get_document", &json!({}))["head_seq"].clone();

    let preview = registry.call(
        &mut context,
        "preview_suggestion",
        &json!({"patch": [
            {"tool": "add_adjustment", "arguments": {"layer_id": "layer_1",
                "adjustment_type": "exposure", "params": {"ev": 0.5}}},
            {"tool": "clone_stamp", "arguments": {"layer_id": "layer_1",
                "points": [[10, 10]], "source_offset": [-8, -8]}},
            {"tool": "get_document", "arguments": {}},
            {"tool": "not_a_tool", "arguments": {}},
            {"tool": "add_filter", "arguments": {"layer_id": "layer_1",
                "filter_name": "gaussian_blur", "params": {"sigma": 999}}}
        ]}),
    );
    assert_eq!(preview["total_steps"], json!(5), "{preview}");
    // 影响范围预估：效果类=整层、修图=区域，汇总取更保守者。
    let steps = preview["steps"].as_array().unwrap();
    assert_eq!(
        steps[0]["affects"],
        json!("layer"),
        "调整作用于整层：{preview}"
    );
    assert_eq!(
        steps[1]["affects"],
        json!("region"),
        "修图只影响笔迹范围：{preview}"
    );
    assert_eq!(preview["estimated_dirty"], json!("layer"), "{preview}");
    let region = steps[1]["estimated_region"].as_array().unwrap();
    assert_eq!(region.len(), 4, "区域步骤应给出包围盒：{preview}");
    assert!(region[2].as_f64().unwrap() > 0.0, "{preview}");
    assert_eq!(preview["invalid_steps"], json!(3), "{preview}");
    assert_eq!(preview["applicable"], json!(false), "含非法步骤时不可应用");
    let steps = preview["steps"].as_array().unwrap();
    assert_eq!(steps[0]["valid"], json!(true));
    assert_eq!(steps[0]["class"], json!("effect"));
    assert_eq!(steps[1]["class"], json!("retouch"));
    assert_eq!(steps[2]["valid"], json!(false), "只读工具应判非法");
    assert_eq!(steps[3]["valid"], json!(false), "未知工具应判非法");
    assert_eq!(steps[4]["valid"], json!(false), "越界参数应判非法");
    // 预览必须无副作用：HEAD 不变。
    let head_after = registry.call(&mut context, "get_document", &json!({}))["head_seq"].clone();
    assert_eq!(
        head_before, head_after,
        "预览不得写入日志：{head_before} vs {head_after}"
    );

    // 全部合法时可应用。
    let good = registry.call(
        &mut context,
        "preview_suggestion",
        &json!({"patch": [{"tool": "create_layer", "arguments": {"layer_id": "layer_2"}}]}),
    );
    assert_eq!(good["applicable"], json!(true), "{good}");
    assert_eq!(good["steps"][0]["class"], json!("structure"));
    assert_eq!(
        good["steps"][0]["affects"],
        json!("document"),
        "新建图层按内核语义是整文档失效：{good}"
    );
    assert_eq!(good["estimated_dirty"], json!("document"));

    // 纯几何补丁：汇总应为 region，并给出并集包围盒。
    let geometric = registry.call(
        &mut context,
        "preview_suggestion",
        &json!({"patch": [
            {"tool": "draw_stroke", "arguments": {"layer_id": "layer_1", "size": 8,
                "data": {"points": [[10, 10], [30, 20]]}}},
            {"tool": "draw_shape", "arguments": {"layer_id": "layer_1",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 40, "y": 40, "w": 16, "h": 16}},
                         "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}}
        ]}),
    );
    assert_eq!(geometric["estimated_dirty"], json!("region"), "{geometric}");
    let merged = geometric["estimated_region"].as_array().unwrap();
    assert!(
        merged[0].as_f64().unwrap() <= 10.0,
        "并集应含第一笔：{geometric}"
    );
    assert!(
        merged[0].as_f64().unwrap() + merged[2].as_f64().unwrap() >= 56.0,
        "并集应含矩形：{geometric}"
    );

    // 用 suggestion_id 预览已记录的建议。
    let suggestion = registry.call(
        &mut context,
        "suggest",
        // 用**已存在**的图层：suggest 会提交原子，引用不存在的目标会被折叠拒绝。
        &json!({"patch": [{"tool": "add_adjustment", "arguments": {"layer_id": "layer_1",
            "adjustment_type": "invert"}}]}),
    );
    let suggestion_id = suggestion["suggestion_id"]
        .as_str()
        .unwrap_or_else(|| panic!("suggest 应返回 suggestion_id：{suggestion}"))
        .to_owned();
    let by_id = registry.call(
        &mut context,
        "preview_suggestion",
        &json!({"suggestion_id": suggestion_id}),
    );
    assert_eq!(by_id["applicable"], json!(true), "{by_id}");
    assert_eq!(by_id["suggestion_id"], json!(suggestion_id));
}

/// Phase 4b 完善：批量接受（个别失败不中断）+ 列表分页/增量轮询。
#[test]
fn batch_accept_and_suggestion_paging() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_batch", 64, 64),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Collab,
        yanshi_server::Profile::Retouch,
    ]);
    let mut context = ToolContext::new(&mut workspace, "doc_batch", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    registry.call(
        &mut context,
        "create_layer",
        &json!({"layer_id": "layer_1"}),
    );

    // 三条可接受的建议 + 一条坏的（未知建议 id）。
    let mut ids = Vec::new();
    for adjustment in ["invert", "saturation", "levels"] {
        let response = registry.call(
            &mut context,
            "suggest",
            &json!({"summary": adjustment, "patch": [{"tool": "add_adjustment",
                "arguments": {"layer_id": "layer_1", "adjustment_type": adjustment,
                              "params": {"amount": 1.2, "black": 0.0, "white": 1.0}}}]}),
        );
        assert_eq!(response["ok"], json!(true), "{response}");
        ids.push(response["suggestion_id"].clone());
    }

    // 分页与增量：limit/offset/total 一致，since_seq 能取增量。
    let all = registry.call(&mut context, "list_suggestions", &json!({"limit": 2}));
    assert_eq!(all["total"], json!(3), "{all}");
    assert_eq!(
        all["suggestions"].as_array().unwrap().len(),
        2,
        "limit 应生效"
    );
    let page2 = registry.call(
        &mut context,
        "list_suggestions",
        &json!({"limit": 2, "offset": 2}),
    );
    assert_eq!(
        page2["suggestions"].as_array().unwrap().len(),
        1,
        "offset 应生效"
    );
    let since_last = registry.call(
        &mut context,
        "list_suggestions",
        &json!({"since_seq": all["suggestions"][1]["seq"].as_u64().unwrap()}),
    );
    assert_eq!(
        since_last["total"],
        json!(1),
        "since_seq 应只返回增量：{since_last}"
    );

    // 批量接受：3 条成功 + 1 条未知（不中断整批）。
    let mut batch = ids.clone();
    batch.push(json!("01ZZZZZZZZZZZZZZZZZZZZZZZZZZ"));
    let accepted = registry.call(
        &mut context,
        "accept_suggestions",
        &json!({"suggestion_ids": batch}),
    );
    assert_eq!(accepted["accepted"], json!(3), "{accepted}");
    assert_eq!(accepted["failed"], json!(1), "{accepted}");
    let results = accepted["results"].as_array().unwrap();
    assert_eq!(results[3]["ok"], json!(false));
    assert_eq!(results[3]["error_code"], json!("reference_not_found"));
    // 全部达成 accepted 状态。
    let done = registry.call(
        &mut context,
        "list_suggestions",
        &json!({"status": "accepted"}),
    );
    assert_eq!(done["total"], json!(3), "{done}");
    // 空列表与超限列表要被拒。
    for payload in [
        json!({"suggestion_ids": []}),
        json!({"suggestion_ids": [1, 2]}),
    ] {
        let response = registry.call(&mut context, "accept_suggestions", &payload);
        assert!(
            response["ok"] == json!(false) || response["accepted"] == json!(0),
            "{payload}: {response}"
        );
    }
}

/// 去雾是「只读估计 + 参数驱动滤镜」两步式：估计值必须能直接喂给滤镜且改变像素。
#[test]
fn dehaze_estimate_then_apply_changes_pixels() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_haze", 96, 96),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Retouch,
    ]);
    let store = workspace.store();
    let mut context = ToolContext::new(&mut workspace, "doc_haze", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    registry.call(
        &mut context,
        "create_layer",
        &json!({"layer_id": "layer_1"}),
    );
    // 雾化画面：彩色场景与白雾按 0.5 混合（暗通道先验要求各通道有差异）。
    registry.call(
        &mut context,
        "draw_shape",
        &json!({"layer_id": "layer_1", "object_id": "dark",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 48, "h": 96}},
                         "color": {"r": 84, "g": 84, "b": 92, "a": 255}}}),
    );
    registry.call(
        &mut context,
        "draw_shape",
        &json!({"layer_id": "layer_1", "object_id": "warm",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 48, "y": 0, "w": 48, "h": 96}},
                         "color": {"r": 218, "g": 160, "b": 148, "a": 255}}}),
    );
    let sample = |context: &mut ToolContext<'_>, x: u32, y: u32| -> [u8; 4] {
        let response = registry.call(
            context,
            "render_region",
            &json!({"region": {"x": 0, "y": 0, "w": 96, "h": 96}, "raw": true}),
        );
        let hash: yanshi_core::BlobHash = response["raw_url"]
            .as_str()
            .unwrap()
            .trim_start_matches("yanshi://blob/")
            .parse()
            .unwrap();
        let pixels = store.get(&hash).unwrap();
        let index = ((y * 96 + x) * 4) as usize;
        [
            pixels[index],
            pixels[index + 1],
            pixels[index + 2],
            pixels[index + 3],
        ]
    };

    // 只读估计：返回可用的大气光与建议强度。
    let estimate = registry.call(&mut context, "estimate_dehaze", &json!({}));
    assert_eq!(estimate["ok"], json!(true), "{estimate}");
    let air = estimate["air"].clone();
    assert!(
        air.as_array().unwrap().len() == 3,
        "air 应为三分量：{estimate}"
    );
    let omega = estimate["suggested"]["omega"].clone();
    assert!(omega.as_f64().unwrap() > 0.5, "建议强度应合理：{estimate}");

    let before = sample(&mut context, 24, 48);
    // 参数驱动滤镜：air 缺失必须被拒（防止滤镜内部做全局估计）。
    let missing_air = registry.call(
        &mut context,
        "add_filter",
        &json!({"layer_id": "layer_1", "filter_name": "dehaze", "params": {"omega": 0.8}}),
    );
    assert_eq!(
        missing_air["error_code"],
        json!("invalid_argument"),
        "{missing_air}"
    );
    let applied = registry.call(
        &mut context,
        "add_filter",
        &json!({"layer_id": "layer_1", "filter_name": "dehaze",
                "params": {"air": air, "omega": omega, "floor": 0.1}}),
    );
    assert_eq!(applied["ok"], json!(true), "{applied}");
    assert_eq!(applied["dirty_kind"], json!("structure"));
    let after = sample(&mut context, 24, 48);
    assert_ne!(before, after, "去雾必须改变像素：{before:?} → {after:?}");
    // 暗部应更暗（对比度回升）。
    assert!(
        after[0] < before[0] + 5,
        "暗部不应变亮：{before:?} → {after:?}"
    );
}

/// 预览必须挡住补丁顺序错误（这类错误若留到接受阶段，会造成部分应用的状态）。
#[test]
fn preview_detects_patch_order_problems() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_order", 64, 64),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Collab,
        yanshi_server::Profile::Retouch,
    ]);
    let mut context = ToolContext::new(&mut workspace, "doc_order", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    registry.call(
        &mut context,
        "create_layer",
        &json!({"layer_id": "existing"}),
    );

    // 顺序错误：先往新图层画，再创建它。
    let wrong = registry.call(
        &mut context,
        "preview_suggestion",
        &json!({"patch": [
            {"tool": "draw_shape", "arguments": {"layer_id": "fresh",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 8, "h": 8}},
                         "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}},
            {"tool": "create_layer", "arguments": {"layer_id": "fresh"}}
        ]}),
    );
    assert_eq!(wrong["applicable"], json!(false), "{wrong}");
    assert_eq!(wrong["steps"][0]["valid"], json!(false), "{wrong}");
    assert!(
        wrong["steps"][0]["error"]
            .as_str()
            .unwrap_or_default()
            .contains("尚未创建"),
        "错误信息应指出顺序问题：{wrong}"
    );
    // patch 自身的模拟不应改变文档。
    let head_before = registry.call(&mut context, "get_document", &json!({}))["head_seq"].clone();

    // 正确顺序：先创建图层再画 → 两步都合法。
    let right = registry.call(
        &mut context,
        "preview_suggestion",
        &json!({"patch": [
            {"tool": "create_layer", "arguments": {"layer_id": "fresh"}},
            {"tool": "draw_shape", "arguments": {"layer_id": "fresh",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 8, "h": 8}},
                         "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}}
        ]}),
    );
    assert_eq!(right["applicable"], json!(true), "{right}");

    // 对象引用：patch 内先创建对象再更新它 → 合法；更新不存在的对象 → 非法。
    let update_existing = registry.call(
        &mut context,
        "preview_suggestion",
        &json!({"patch": [
            {"tool": "add_adjustment", "arguments": {"layer_id": "existing", "object_id": "fx1",
                "adjustment_type": "exposure", "params": {"ev": 0.2}}},
            {"tool": "update_adjustment", "arguments": {"object_id": "fx1", "params": {"ev": 0.4}}}
        ]}),
    );
    assert_eq!(
        update_existing["applicable"],
        json!(true),
        "{update_existing}"
    );
    let update_missing = registry.call(
        &mut context,
        "preview_suggestion",
        &json!({"patch": [
            {"tool": "update_adjustment", "arguments": {"object_id": "nope", "params": {"ev": 0.4}}}
        ]}),
    );
    assert_eq!(
        update_missing["applicable"],
        json!(false),
        "{update_missing}"
    );
    assert!(
        update_missing["steps"][0]["error"]
            .as_str()
            .unwrap_or_default()
            .contains("不存在"),
        "{update_missing}"
    );
    // 预览全程无副作用。
    let head_after = registry.call(&mut context, "get_document", &json!({}))["head_seq"].clone();
    assert_eq!(head_before, head_after, "预览不得写入日志");
}

/// 建议冲突检测：同图层/同对象/区域相交的待处理建议应被标出；不重叠的不应误报。
#[test]
fn suggestion_conflicts_are_reported() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_conflict", 128, 128),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Collab,
        yanshi_server::Profile::Retouch,
    ]);
    let mut context = ToolContext::new(&mut workspace, "doc_conflict", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    registry.call(&mut context, "create_layer", &json!({"layer_id": "L"}));

    // 两条都在 L 层上做整层调整 → 必然冲突（作用于整层）。
    for summary in ["提亮", "压暗"] {
        registry.call(
            &mut context,
            "suggest",
            &json!({"summary": summary, "patch": [{"tool": "add_adjustment",
                "arguments": {"layer_id": "L", "adjustment_type": "exposure",
                              "params": {"ev": if summary == "提亮" { 0.3 } else { -0.3 }}}}]}),
        );
    }
    // 一条在远角画小矩形（与上面的整层改动同层 → 仍冲突，因为一方是整层）。
    registry.call(
        &mut context,
        "suggest",
        &json!({"summary": "角落画点", "patch": [{"tool": "draw_shape",
            "arguments": {"layer_id": "L", "data": {"geometry": {"kind": "rect",
                "bbox": {"x": 100, "y": 100, "w": 8, "h": 8}},
                "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}}]}),
    );

    let listed = registry.call(&mut context, "list_suggestions", &json!({}));
    let conflicts = listed["conflicts"].as_array().unwrap();
    assert!(
        !conflicts.is_empty(),
        "同层的整层改动应被标为冲突：{listed}"
    );
    assert!(
        conflicts.iter().any(|conflict| conflict["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("整层")),
        "冲突原因应说明整层作用：{listed}"
    );
}

/// 反例：互不相交、对象不同的区域改动不应被误报为冲突。
#[test]
fn disjoint_suggestions_do_not_conflict() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_no_conflict", 128, 128),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Collab,
    ]);
    let mut context = ToolContext::new(&mut workspace, "doc_no_conflict", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    registry.call(&mut context, "create_layer", &json!({"layer_id": "L"}));
    // 不传 object_id（带了会走 supersede 语义、要求对象已存在），靠**区域不相交**区分。
    for (summary, x) in [("left", 8.0), ("right", 80.0)] {
        let response = registry.call(
            &mut context,
            "suggest",
            &json!({"summary": summary, "patch": [{"tool": "draw_shape",
                "arguments": {"layer_id": "L",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": x, "y": 8, "w": 16, "h": 16}},
                             "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}}]}),
        );
        assert_eq!(response["ok"], json!(true), "{response}");
    }
    let listed = registry.call(&mut context, "list_suggestions", &json!({}));
    assert_eq!(
        listed["conflicts"].as_array().unwrap().len(),
        0,
        "区域不相交、对象不同的建议不应误报冲突：{listed}"
    );
}

/// 接受建议前必须**全量预检**：任一步不合法就整体拒绝，日志不得被写入（避免部分应用）。
#[test]
fn accept_preflights_the_whole_patch_before_writing() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_preflight", 64, 64),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Collab,
        yanshi_server::Profile::Retouch,
    ]);
    let mut context = ToolContext::new(&mut workspace, "doc_preflight", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    registry.call(&mut context, "create_layer", &json!({"layer_id": "L"}));

    // 第 1 步合法、第 2 步是只读工具 → 预检整体拒绝。
    // （注意：引用不存在的图层连 suggest 都提交不了 —— 折叠层的引用校验会挡下，
    //   因此这里用「只读步骤」构造，语义无歧义。）
    let suggestion = registry.call(
        &mut context,
        "suggest",
        &json!({"patch": [
            {"tool": "add_adjustment", "arguments": {"layer_id": "L",
                "adjustment_type": "exposure", "params": {"ev": 0.3}}},
            {"tool": "get_document", "arguments": {}}
        ]}),
    );
    let suggestion_id = suggestion["suggestion_id"]
        .as_str()
        .unwrap_or_else(|| panic!("suggest 应成功：{suggestion}"))
        .to_owned();
    let head_before = registry.call(&mut context, "get_document", &json!({}))["head_seq"].clone();
    let objects_before = registry.call(&mut context, "list_objects", &json!({}))["count"].clone();

    let rejected = registry.call(
        &mut context,
        "accept_suggestion",
        &json!({"suggestion_id": suggestion_id}),
    );
    assert_eq!(rejected["ok"], json!(false), "{rejected}");
    assert_eq!(
        rejected["error_code"],
        json!("invalid_argument"),
        "{rejected}"
    );
    assert!(
        rejected["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("只读工具"),
        "应说明是只读步骤：{rejected}"
    );

    // 关键断言：**一步都没写**（HEAD 与对象数都不变），不留下部分应用状态。
    let head_after = registry.call(&mut context, "get_document", &json!({}))["head_seq"].clone();
    let objects_after = registry.call(&mut context, "list_objects", &json!({}))["count"].clone();
    assert_eq!(
        head_before, head_after,
        "预检失败不得写入任何原子：{head_before} vs {head_after}"
    );
    assert_eq!(objects_before, objects_after, "预检失败不得创建对象");

    // 完全合法的补丁（两步都在已存在的图层上）应完整落地。
    let good = registry.call(
        &mut context,
        "suggest",
        &json!({"patch": [
            {"tool": "add_adjustment", "arguments": {"layer_id": "L",
                "adjustment_type": "exposure", "params": {"ev": 0.4}}},
            {"tool": "add_filter", "arguments": {"layer_id": "L",
                "filter_name": "vignette", "params": {"strength": 0.3}}}
        ]}),
    );
    let good_id = good["suggestion_id"]
        .as_str()
        .unwrap_or_else(|| panic!("合法补丁的 suggest 应成功：{good}"))
        .to_owned();
    let accepted = registry.call(
        &mut context,
        "accept_suggestion",
        &json!({"suggestion_id": good_id}),
    );
    assert_eq!(accepted["ok"], json!(true), "{accepted}");
    assert_eq!(
        accepted["applied_atom_ids"].as_array().unwrap().len(),
        2,
        "{accepted}"
    );
    let head_final = registry.call(&mut context, "get_document", &json!({}))["head_seq"]
        .as_u64()
        .unwrap();
    assert!(
        head_final > head_before.as_u64().unwrap(),
        "合法补丁应真正落地：{head_before} → {head_final}"
    );
}

/// 回归：补丁里「先创建图层、再在同一条补丁内绘制」必须能记录并成功接受。
///
/// 修复前这类建议**连 suggest 都提交不了**：`validate_secondary_refs` 会把 payload 中
/// 嵌套步骤里的 `layer_id` 当成该原子自身的引用（refs 是递归收集的），
/// 于是按「必须已存在」拒绝了整条建议。协作原子不产生状态效果，豁免该校验。
#[test]
fn patch_can_create_and_use_a_layer_within_one_suggestion() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_create_use", 64, 64),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Collab,
    ]);
    let store = workspace.store();
    let mut context = ToolContext::new(&mut workspace, "doc_create_use", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    registry.call(&mut context, "create_layer", &json!({"layer_id": "base"}));

    let suggestion = registry.call(
        &mut context,
        "suggest",
        &json!({"summary": "新建图层并画一个方块", "patch": [
            {"tool": "create_layer", "arguments": {"layer_id": "fresh"}},
            {"tool": "draw_shape", "arguments": {"layer_id": "fresh",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 8, "y": 8, "w": 16, "h": 16}},
                         "color": {"r": 255, "g": 0, "b": 0, "a": 255}}}}
        ]}),
    );
    assert_eq!(
        suggestion["ok"],
        json!(true),
        "先建后画应能记录：{suggestion}"
    );
    let suggestion_id = suggestion["suggestion_id"].as_str().unwrap().to_owned();

    // 预览也应认为可应用（顺序正确）。
    let preview = registry.call(
        &mut context,
        "preview_suggestion",
        &json!({"suggestion_id": suggestion_id}),
    );
    assert_eq!(preview["applicable"], json!(true), "{preview}");

    let accepted = registry.call(
        &mut context,
        "accept_suggestion",
        &json!({"suggestion_id": suggestion_id}),
    );
    assert_eq!(accepted["ok"], json!(true), "{accepted}");
    assert_eq!(
        accepted["applied_atom_ids"].as_array().unwrap().len(),
        2,
        "{accepted}"
    );

    // 像素真的出现了：新图层上的红色方块。
    let response = registry.call(
        &mut context,
        "render_region",
        &json!({"region": {"x": 0, "y": 0, "w": 64, "h": 64}, "raw": true}),
    );
    let hash: yanshi_core::BlobHash = response["raw_url"]
        .as_str()
        .unwrap()
        .trim_start_matches("yanshi://blob/")
        .parse()
        .unwrap();
    let pixels = store.get(&hash).unwrap();
    let index = ((16 * 64 + 16) * 4) as usize;
    assert!(
        pixels[index] > 200 && pixels[index + 1] < 60,
        "新建图层上的红方块应可见：{:?}",
        &pixels[index..index + 4]
    );
}

/// 建议接受的**状态闸门**（设计未规定，此处固定行为防 footgun）：
/// 已接受的建议不得重复施加效果；已拒绝的建议不得再被接受。
#[test]
fn accept_is_not_reapplied_and_rejected_suggestions_cannot_be_accepted() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_gate", 64, 64),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Collab,
        yanshi_server::Profile::Retouch,
    ]);
    let store = workspace.store();
    let mut context = ToolContext::new(&mut workspace, "doc_gate", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    registry.call(&mut context, "create_layer", &json!({"layer_id": "L"}));
    registry.call(
        &mut context,
        "draw_shape",
        &json!({"layer_id": "L", "object_id": "base",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 64, "h": 64}},
                         "color": {"r": 100, "g": 100, "b": 100, "a": 255}}}),
    );
    let sample = |context: &mut ToolContext<'_>| -> [u8; 4] {
        let response = registry.call(
            context,
            "render_region",
            &json!({"region": {"x": 0, "y": 0, "w": 64, "h": 64}, "raw": true}),
        );
        let hash: yanshi_core::BlobHash = response["raw_url"]
            .as_str()
            .unwrap()
            .trim_start_matches("yanshi://blob/")
            .parse()
            .unwrap();
        let pixels = store.get(&hash).unwrap();
        let index = ((32 * 64 + 32) * 4) as usize;
        [
            pixels[index],
            pixels[index + 1],
            pixels[index + 2],
            pixels[index + 3],
        ]
    };

    // 先接受一次：曝光 +1EV。
    let suggestion = registry.call(
        &mut context,
        "suggest",
        &json!({"patch": [{"tool": "add_adjustment", "arguments": {"layer_id": "L",
            "adjustment_type": "exposure", "params": {"ev": 1.0}}}]}),
    );
    let suggestion_id = suggestion["suggestion_id"].as_str().unwrap().to_owned();
    let first = registry.call(
        &mut context,
        "accept_suggestion",
        &json!({"suggestion_id": suggestion_id}),
    );
    assert_eq!(first["ok"], json!(true), "{first}");
    assert_eq!(
        first["applied_atom_ids"].as_array().unwrap().len(),
        1,
        "{first}"
    );
    let after_first = sample(&mut context);

    // 再次接受同一条：不得重复施加（否则像素会再亮一档）。
    let second = registry.call(
        &mut context,
        "accept_suggestion",
        &json!({"suggestion_id": suggestion_id}),
    );
    assert_eq!(second["ok"], json!(true), "{second}");
    assert_eq!(second["already_accepted"], json!(true), "{second}");
    assert_eq!(
        second["applied_atom_ids"].as_array().unwrap().len(),
        0,
        "重复接受不应再写原子：{second}"
    );
    let after_second = sample(&mut context);
    assert_eq!(
        after_first, after_second,
        "重复接受不得改变像素：{after_first:?} → {after_second:?}"
    );

    // 已拒绝的建议不得被接受。
    let rejected = registry.call(
        &mut context,
        "suggest",
        &json!({"patch": [{"tool": "add_adjustment", "arguments": {"layer_id": "L",
            "adjustment_type": "invert"}}]}),
    );
    let rejected_id = rejected["suggestion_id"].as_str().unwrap().to_owned();
    registry.call(
        &mut context,
        "reject_suggestion",
        &json!({"suggestion_id": rejected_id, "reason": "先不做"}),
    );
    let refused = registry.call(
        &mut context,
        "accept_suggestion",
        &json!({"suggestion_id": rejected_id}),
    );
    assert_eq!(refused["ok"], json!(false), "{refused}");
    assert_eq!(
        refused["error_code"],
        json!("precondition_failed"),
        "{refused}"
    );
    assert!(
        refused["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("已被拒绝"),
        "{refused}"
    );

    // 批量接受：重复 id 不应造成二次施加效果（两次都 ok，第二次是幂等返回）。
    let batch = registry.call(
        &mut context,
        "accept_suggestions",
        &json!({"suggestion_ids": [suggestion_id, suggestion_id]}),
    );
    assert_eq!(
        batch["accepted"],
        json!(2),
        "两次都应成功（第二次是幂等返回）：{batch}"
    );
    assert_eq!(batch["failed"], json!(0), "{batch}");
    let after_batch = sample(&mut context);
    assert_eq!(after_first, after_batch, "批量重复接受同样不得改变像素");
}

/// 顺序巡检：补丁里删掉图层后再引用它，预览必须判为非法（删除会把目标移出已知集合）。
#[test]
fn preview_flags_references_after_deletion_in_the_same_patch() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_delete_order", 64, 64),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Collab,
        yanshi_server::Profile::Retouch,
    ]);
    let mut context =
        ToolContext::new(&mut workspace, "doc_delete_order", "human:1", "session:web")
            .with_owner(true)
            .with_wait_for_render(true, 500);
    registry.call(&mut context, "create_layer", &json!({"layer_id": "L"}));
    registry.call(&mut context, "create_layer", &json!({"layer_id": "doomed"}));

    let preview = registry.call(
        &mut context,
        "preview_suggestion",
        &json!({"patch": [
            {"tool": "delete_layer", "arguments": {"layer_id": "doomed"}},
            {"tool": "add_adjustment", "arguments": {"layer_id": "doomed",
                "adjustment_type": "invert"}}
        ]}),
    );
    assert_eq!(preview["applicable"], json!(false), "{preview}");
    assert_eq!(
        preview["steps"][0]["valid"],
        json!(true),
        "删除本身合法：{preview}"
    );
    assert_eq!(
        preview["steps"][1]["valid"],
        json!(false),
        "删除后引用应非法：{preview}"
    );
    assert!(
        preview["steps"][1]["error"]
            .as_str()
            .unwrap_or_default()
            .contains("尚未创建"),
        "应指出引用问题：{preview}"
    );
    // 预览无副作用。
    let head = registry.call(&mut context, "get_document", &json!({}))["head_seq"].clone();
    assert!(head.as_u64().unwrap() > 0);
}

/// 标注状态机（设计 4.6 只列三态，转换规则未规定 → 此处固定行为）：
/// 幂等解决、禁止 resolved↔rejected 直接翻转、可重开、删除是软删除且允许从任意状态执行。
#[test]
fn annotation_lifecycle_rules_are_enforced() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_life", 64, 64),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Annotation,
        yanshi_server::Profile::Collab,
    ]);
    let mut context = ToolContext::new(&mut workspace, "doc_life", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    let create = |context: &mut ToolContext<'_>, content: &str| -> String {
        let response = registry.call(
            context,
            "create_annotation",
            &json!({"type": "region", "content": content, "intent": "modify",
                    "target": {"type": "region", "bbox": {"x": 0, "y": 0, "w": 8, "h": 8}}}),
        );
        response["annotation_id"].as_str().unwrap().to_owned()
    };

    // 幂等：重复 resolve 不新增版本、不动 revision。
    let first = create(&mut context, "第一次");
    let resolved = registry.call(
        &mut context,
        "resolve_annotation",
        &json!({"annotation_id": first}),
    );
    assert_eq!(resolved["ok"], json!(true), "{resolved}");
    let revision = resolved["annotation"]["revision"].clone();
    let again = registry.call(
        &mut context,
        "resolve_annotation",
        &json!({"annotation_id": first}),
    );
    assert_eq!(again["ok"], json!(true), "{again}");
    assert_eq!(
        again["annotation"]["revision"], revision,
        "重复解决应幂等：{again}"
    );

    // 禁止直接用 reject 翻转一个已解决的标注。
    let flipped = registry.call(
        &mut context,
        "reject_annotation",
        &json!({"annotation_id": first}),
    );
    assert_eq!(
        flipped["error_code"],
        json!("precondition_failed"),
        "{flipped}"
    );
    assert!(
        flipped["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("重开"),
        "错误应提示先重开：{flipped}"
    );

    // 重开后可以再解决（reopen 必须清空 resolved_by/resolved_at）。
    let reopened = registry.call(
        &mut context,
        "update_annotation",
        &json!({"annotation_id": first, "status": "pending", "content": "重开后的描述"}),
    );
    assert_eq!(reopened["ok"], json!(true), "{reopened}");
    assert_eq!(
        reopened["annotation"]["status"],
        json!("pending"),
        "{reopened}"
    );
    assert!(
        reopened["annotation"]["resolved_by"].is_null(),
        "重开应清空 resolved_by"
    );
    assert!(
        reopened["annotation"]["resolved_at"].is_null(),
        "重开应清空 resolved_at"
    );
    assert_eq!(reopened["annotation"]["content"], json!("重开后的描述"));
    assert_eq!(reopened["pending"], json!(1));

    // update_annotation 不接受其它状态（不得绕过状态机）。
    let bypass = registry.call(
        &mut context,
        "update_annotation",
        &json!({"annotation_id": first, "status": "resolved"}),
    );
    assert_eq!(bypass["error_code"], json!("invalid_argument"), "{bypass}");

    // 软删除：允许从任意状态执行，记录仍可取回（append-only 资产不可回滚删除）。
    let second = create(&mut context, "待删除");
    registry.call(
        &mut context,
        "resolve_annotation",
        &json!({"annotation_id": second}),
    );
    let deleted = registry.call(
        &mut context,
        "delete_annotation",
        &json!({"annotation_id": second}),
    );
    assert_eq!(
        deleted["ok"],
        json!(true),
        "已解决的标注应可删除：{deleted}"
    );
    let fetched = registry.call(
        &mut context,
        "get_annotation",
        &json!({"annotation_id": second}),
    );
    assert_eq!(fetched["ok"], json!(true), "软删除后仍应可取回：{fetched}");
    assert_eq!(fetched["annotation"]["status"], json!("rejected"));
    assert_eq!(
        fetched["annotation"]["content"],
        json!(""),
        "删除会清空内容"
    );
    let pending = registry.call(
        &mut context,
        "list_annotations",
        &json!({"status": "pending"}),
    );
    assert_eq!(pending["pending"], json!(1), "只剩重开的那条：{pending}");
}

/// 建议优先级：写入、校验、并在列表里**按优先级排序**（同优先级按先后）。
#[test]
fn suggestion_priority_is_validated_and_orders_the_list() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_prio", 64, 64),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Collab,
        yanshi_server::Profile::Retouch,
    ]);
    let mut context = ToolContext::new(&mut workspace, "doc_prio", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    registry.call(&mut context, "create_layer", &json!({"layer_id": "L"}));

    // 故意按「低 → 高 → 缺省」的顺序创建，检验排序而非插入顺序。
    for (summary, priority) in [("低", Some(1)), ("高", Some(9)), ("缺省", None)] {
        let mut payload = json!({"summary": summary, "patch": [{"tool": "add_adjustment",
            "arguments": {"layer_id": "L", "adjustment_type": "invert"}}]});
        if let Some(priority) = priority {
            payload["priority"] = json!(priority);
        }
        let response = registry.call(&mut context, "suggest", &payload);
        assert_eq!(response["ok"], json!(true), "{response}");
        assert_eq!(
            response["priority"],
            json!(priority.unwrap_or(5)),
            "响应应回显生效的优先级：{response}"
        );
    }
    // 越界优先级必须被拒。
    for invalid in [-1, 10] {
        let response = registry.call(
            &mut context,
            "suggest",
            &json!({"priority": invalid, "patch": [{"tool": "add_adjustment",
                "arguments": {"layer_id": "L", "adjustment_type": "invert"}}]}),
        );
        assert_eq!(
            response["error_code"],
            json!("invalid_argument"),
            "{invalid}: {response}"
        );
        assert!(
            response["context"]["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("0..9"),
            "{invalid}: {response}"
        );
    }

    let listed = registry.call(&mut context, "list_suggestions", &json!({}));
    let summaries: Vec<String> = listed["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["summary"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(
        summaries,
        vec!["高".to_owned(), "缺省".to_owned(), "低".to_owned()],
        "应按优先级降序（同优先级按先后）：{listed}"
    );
    let priorities: Vec<i64> = listed["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["priority"].as_i64().unwrap_or(0))
        .collect();
    assert_eq!(priorities, vec![9, 5, 1], "{listed}");
}

/// 标注 ↔ 建议的关联查询：`list_annotations(suggestion_id=...)` 应能直接筛出引用该建议的标注。
#[test]
fn annotations_can_be_filtered_by_suggestion() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_link", 64, 64),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Annotation,
        yanshi_server::Profile::Collab,
    ]);
    let mut context = ToolContext::new(&mut workspace, "doc_link", "human:1", "session:web")
        .with_owner(true)
        .with_wait_for_render(true, 500);
    registry.call(&mut context, "create_layer", &json!({"layer_id": "L"}));
    let suggestion = registry.call(
        &mut context,
        "suggest",
        &json!({"summary": "提亮", "patch": [{"tool": "add_adjustment",
            "arguments": {"layer_id": "L", "adjustment_type": "exposure", "params": {"ev": 0.5}}}]}),
    );
    let suggestion_id = suggestion["suggestion_id"].as_str().unwrap().to_owned();

    // 两条引用该建议、一条不引用。
    for (content, linked) in [("一", true), ("二", true), ("三", false)] {
        let mut payload = json!({"type": "region", "content": content, "intent": "modify",
            "target": {"type": "region", "bbox": {"x": 0, "y": 0, "w": 8, "h": 8}}});
        if linked {
            payload["suggestion_id"] = json!(suggestion_id);
        }
        let response = registry.call(&mut context, "create_annotation", &payload);
        assert_eq!(response["ok"], json!(true), "{response}");
    }

    let linked = registry.call(
        &mut context,
        "list_annotations",
        &json!({"suggestion_id": suggestion_id}),
    );
    assert_eq!(
        linked["count"],
        json!(2),
        "应只筛出引用该建议的标注：{linked}"
    );
    let all = registry.call(&mut context, "list_annotations", &json!({}));
    assert_eq!(all["count"], json!(3), "{all}");
    // 组合过滤：状态 + 建议。
    let pending_linked = registry.call(
        &mut context,
        "list_annotations",
        &json!({"suggestion_id": suggestion_id, "status": "pending"}),
    );
    assert_eq!(pending_linked["count"], json!(2), "{pending_linked}");

    // 接受建议后，这两条应被置为 resolved，因此 pending+suggestion_id 筛出 0 条。
    registry.call(
        &mut context,
        "accept_suggestion",
        &json!({"suggestion_id": suggestion_id}),
    );
    let after = registry.call(
        &mut context,
        "list_annotations",
        &json!({"suggestion_id": suggestion_id, "status": "pending"}),
    );
    assert_eq!(after["count"], json!(0), "{after}");
    let resolved = registry.call(
        &mut context,
        "list_annotations",
        &json!({"suggestion_id": suggestion_id, "status": "resolved"}),
    );
    assert_eq!(resolved["count"], json!(2), "{resolved}");
}

/// 批量拒绝（与批量接受对称）：逐条不中断、幂等、且不接受与拒绝互相矛盾。
#[test]
fn batch_reject_is_symmetric_with_batch_accept() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_batch_reject", 64, 64),
            "human:1",
            "session:web",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&[
        yanshi_server::Profile::Core,
        yanshi_server::Profile::Collab,
        yanshi_server::Profile::Annotation,
        yanshi_server::Profile::Retouch,
    ]);
    let mut context =
        ToolContext::new(&mut workspace, "doc_batch_reject", "human:1", "session:web")
            .with_owner(true)
            .with_wait_for_render(true, 500);
    registry.call(&mut context, "create_layer", &json!({"layer_id": "L"}));

    let mut ids = Vec::new();
    for summary in ["a", "b", "accepted"] {
        let response = registry.call(
            &mut context,
            "suggest",
            &json!({"summary": summary, "patch": [{"tool": "add_adjustment",
                "arguments": {"layer_id": "L", "adjustment_type": "invert"}}]}),
        );
        ids.push(response["suggestion_id"].as_str().unwrap().to_owned());
    }
    // 第三条先接受，之后批量拒绝它应当失败（precondition_failed）。
    let accepted = registry.call(
        &mut context,
        "accept_suggestion",
        &json!({"suggestion_id": ids[2]}),
    );
    assert_eq!(accepted["ok"], json!(true), "{accepted}");

    let mut batch: Vec<serde_json::Value> = ids.iter().map(|id| json!(id)).collect();
    batch.push(json!("01ZZZZZZZZZZZZZZZZZZZZZZZZZZ"));
    let rejected = registry.call(
        &mut context,
        "reject_suggestions",
        &json!({"suggestion_ids": batch, "reason": "本轮不做"}),
    );
    assert_eq!(
        rejected["rejected"],
        json!(2),
        "两条应被拒绝（第三条已接受，拒不了）: {rejected}"
    );
    assert_eq!(
        rejected["failed"],
        json!(2),
        "已接受的一条 + 未知 id 各算一次失败：{rejected}"
    );
    let results = rejected["results"].as_array().unwrap();
    assert_eq!(results[0]["ok"], json!(true));
    assert_eq!(results[2]["ok"], json!(false));
    assert_eq!(
        results[2]["error_code"],
        json!("precondition_failed"),
        "{rejected}"
    );
    assert_eq!(
        results[3]["error_code"],
        json!("reference_not_found"),
        "{rejected}"
    );

    // 幂等：单条重复拒绝不再写原子。
    let again = registry.call(
        &mut context,
        "reject_suggestion",
        &json!({"suggestion_id": ids[0], "reason": "再来一次"}),
    );
    assert_eq!(again["ok"], json!(true), "{again}");
    assert_eq!(again["already_rejected"], json!(true), "{again}");
    assert_eq!(
        again["rejected_annotations"].as_array().unwrap().len(),
        0,
        "重复拒绝不应再改动标注：{again}"
    );

    // 列表状态：两条 rejected、一条 accepted。
    let listed = registry.call(&mut context, "list_suggestions", &json!({}));
    let statuses: Vec<String> = listed["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["status"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(statuses.contains(&"accepted".to_owned()), "{listed}");
    assert_eq!(
        statuses
            .iter()
            .filter(|status| *status == "rejected")
            .count(),
        2,
        "{listed}"
    );
}

/// 多级撤销/重做：连续 revert 两笔 → 两笔都消失；再 reapply → 按原序恢复。
///
/// 语义依据 `fold.rs`：`revert(revert(x)) ≡ reapply(x)`，因此**重做的目标是原始原子**。
/// 这是查看器「撤销/重做」按钮背后的完整行为，逐像素在服务端验证（不依赖浏览器）。
#[test]
fn multi_step_undo_and_redo_restore_pixels_in_order() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_flow", 512, 512),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = ToolRegistry::full();
    let mut ctx = ToolContext::new(&mut workspace, "doc_flow", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 2_000);
    let call = |ctx: &mut ToolContext<'_>, tool: &str, args: serde_json::Value| {
        registry.call(ctx, tool, &args)
    };

    call(&mut ctx, "create_layer", json!({"layer_id": "L"}));
    let atom_a = call(
        &mut ctx,
        "draw_stroke",
        json!({"layer_id": "L", "object_id": "oa",
               "data": {"points": [[100.0, 100.0], [140.0, 100.0]], "size": 20.0,
                        "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
    )["atom_id"]
        .as_str()
        .expect("draw_stroke 应返回 atom_id")
        .to_owned();
    let atom_b = call(
        &mut ctx,
        "draw_stroke",
        json!({"layer_id": "L", "object_id": "ob",
               "data": {"points": [[300.0, 300.0], [340.0, 300.0]], "size": 20.0,
                        "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
    )["atom_id"]
        .as_str()
        .expect("draw_stroke 应返回 atom_id")
        .to_owned();

    let ink = |ctx: &mut ToolContext<'_>, x: f64, y: f64| -> usize {
        let (_, _, pixels) = ctx
            .workspace
            .render_region_raw("doc_flow", Bbox::new(x, y, 120.0, 120.0))
            .expect("区域渲染应成功");
        pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 8 && (pixel[0] < 200 || pixel[1] < 200 || pixel[2] < 200))
            .count()
    };

    assert!(ink(&mut ctx, 60.0, 60.0) > 0, "第一笔应可见");
    assert!(ink(&mut ctx, 260.0, 260.0) > 0, "第二笔应可见");

    // 撤销第二笔 → 只剩第一笔。
    assert_eq!(
        call(&mut ctx, "revert", json!({"atom_id": atom_b}))["ok"],
        json!(true)
    );
    assert!(ink(&mut ctx, 60.0, 60.0) > 0, "撤销第二笔后第一笔仍在");
    assert_eq!(ink(&mut ctx, 260.0, 260.0), 0, "撤销第二笔后第二笔消失");

    // 再撤销第一笔 → 两笔都消失（多级撤销）。
    assert_eq!(
        call(&mut ctx, "revert", json!({"atom_id": atom_a}))["ok"],
        json!(true)
    );
    assert_eq!(ink(&mut ctx, 60.0, 60.0), 0, "多级撤销后第一笔也消失");
    assert_eq!(ink(&mut ctx, 260.0, 260.0), 0);

    // 重做第一笔（reapply 目标是**原始原子**）→ 只有第一笔回来。
    assert_eq!(
        call(&mut ctx, "reapply", json!({"atom_id": atom_a}))["ok"],
        json!(true)
    );
    assert!(ink(&mut ctx, 60.0, 60.0) > 0, "重做后第一笔恢复");
    assert_eq!(ink(&mut ctx, 260.0, 260.0), 0, "未重做的第二笔仍为空");
}

/// `revert_to`（设计 10.2 历史组，13.2 历史浏览用）：回到某原子时刻的像素状态。
#[test]
fn revert_to_restores_the_state_at_that_atom() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_flow", 512, 512),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = ToolRegistry::full();
    let mut ctx = ToolContext::new(&mut workspace, "doc_flow", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 2_000);
    let call = |ctx: &mut ToolContext<'_>, tool: &str, args: serde_json::Value| {
        registry.call(ctx, tool, &args)
    };

    let create_layer = call(&mut ctx, "create_layer", json!({"layer_id": "L"}));
    let layer_atom = create_layer["atom_id"].as_str().unwrap().to_owned();
    call(
        &mut ctx,
        "draw_stroke",
        json!({"layer_id": "L", "object_id": "o1",
               "data": {"points": [[100.0, 100.0], [140.0, 100.0]], "size": 20.0,
                        "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
    );

    let ink = |ctx: &mut ToolContext<'_>| -> usize {
        let (_, _, pixels) = ctx
            .workspace
            .render_region_raw("doc_flow", Bbox::new(60.0, 60.0, 120.0, 120.0))
            .expect("区域渲染应成功");
        pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 8 && (pixel[0] < 200 || pixel[1] < 200 || pixel[2] < 200))
            .count()
    };
    assert!(ink(&mut ctx) > 0, "笔画应可见");

    // 回到「刚创建图层」那一刻：笔画还未提交，画布应为空。
    assert_eq!(
        call(&mut ctx, "revert_to", json!({"atom_id": layer_atom}))["ok"],
        json!(true)
    );
    assert_eq!(ink(&mut ctx), 0, "回到该时刻后笔画应消失");
}
