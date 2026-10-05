//! **服务端阶段耗时（P2）与在飞操作/取消（P1）** —— 外部测试报告的两条发现。
//!
//! 背景（报告原话）：
//! * **P2**：响应里没有任何服务端阶段耗时 ⇒ 没人知道一次慢的 `brush_stroke` 把时间花在哪
//!   （dab 生成 / 合成 / 日志持久化）⇒ 测试方只能从外部实验反推模型；
//! * **P1**：客户端超时**不会**让服务端停下来 ⇒ 文档仍被独占、用户分不清"慢"与"挂死" ⇒
//!   重试造成**重复落笔**（真实事故：seq 477/478 的孤儿原子）；
//!   相关的还有：`scatter_strokes` 在 `batch` 里 0 s 返回，真正的合成发生在下一次渲染 ⇒
//!   日志里的"每笔耗时"是假的。
//!
//! **本文件的判据全部是语义断言** ✓ —— 没有一条依赖挂钟阈值 ✓
//!（"耗时 > 0"是"这一相真的被量到了"的存在性断言 ✓，不是性能门槛 ✓）。

use serde_json::{json, Value};
use yanshi_server::{
    DocumentSettings, NewDocument, Profile, Role, ToolContext, ToolRegistry, Workspace,
};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_timing_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

/// 落盘工作区 ＋ 仓库里的 `assets/`（`brush_stroke` 需要真实笔刷 ✓）。
fn brush_workspace(root: &std::path::Path, doc_id: &str) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new(doc_id, 320, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call_on(workspace: &mut Workspace, doc_id: &str, tool: &str, args: Value) -> Value {
    call_on_with(workspace, doc_id, tool, args, true, Role::Owner)
}

/// 带 `wait_for_render` / 角色的一次调用（P1 的推迟与权限判据要用 ✓）。
fn call_on_with(
    workspace: &mut Workspace,
    doc_id: &str,
    tool: &str,
    args: Value,
    wait_for_render: bool,
    role: Role,
) -> Value {
    let mut ctx = ToolContext::new(workspace, doc_id, "human:1", "session:test")
        .with_owner(true)
        .with_role(role)
        .with_wait_for_render(wait_for_render, 4_000);
    registry().call(&mut ctx, tool, &args)
}

/// `timings` 的字段名（10.1 的附加对象 ✓）。
const PHASES: [&str; 5] = ["prep_ms", "raster_ms", "dirty_ms", "fold_ms", "log_ms"];

fn sum_of_phases(timings: &Value) -> f64 {
    PHASES
        .iter()
        .map(|key| {
            timings[*key]
                .as_f64()
                .unwrap_or_else(|| panic!("timings.{key} 必须是数字：{timings}"))
        })
        .sum::<f64>()
        + timings["other_ms"]
            .as_f64()
            .unwrap_or_else(|| panic!("timings.other_ms 必须是数字：{timings}"))
}

// ---------------------------------------------------------------------------
// P2：阶段耗时
// ---------------------------------------------------------------------------

/// **判据（P2）**：写路径响应带 `timings` ✓，且**各相之和 == 报告的总时长** ✓。
///
/// **怎么变红** ✗（两条都实测过 ✓）：
/// * 去掉附 `timings` 的那两处（`dispatch` 与 `ToolRegistry::call` ✓）⇒ 键缺失 ⇒ 第一条断言失败 ✓；
/// * 把残差算错（`other_ms` 直接用 `total` 而不是 `total - 已量各相` ✓）⇒
///   五项之和**大于** `total_ms` ⇒ 第三条断言失败 ✓（实测 sum=2031.8 vs total=1897.7 ✓）。
#[test]
fn a_write_response_carries_a_self_consistent_timing_breakdown() {
    let root = temp_dir("timings");
    let mut workspace = brush_workspace(&root, "doc_timings");
    assert_eq!(
        call_on(
            &mut workspace,
            "doc_timings",
            "create_layer",
            json!({"layer_id": "L"})
        )["ok"],
        json!(true)
    );
    let made = call_on(
        &mut workspace,
        "doc_timings",
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "b1", "brush": "2B_pencil", "size": 24,
            "points": [[20.0, 40.0, 0.4], [160.0, 120.0, 0.9], [300.0, 60.0, 0.4]]
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");

    // ① 键必须齐全 ✓（`other_ms` 是残差 ✓，所以它也在里边 ✓）。
    let timings = made
        .get("timings")
        .unwrap_or_else(|| panic!("写路径响应必须带 timings：{made}"));
    for key in PHASES.iter().chain(["total_ms", "other_ms"].iter()) {
        assert!(
            timings[*key].as_f64().is_some(),
            "timings.{key} 应当是数字：{timings}"
        );
        assert!(
            timings[*key].as_f64().unwrap() >= 0.0,
            "timings.{key} 不该是负数：{timings}"
        );
    }
    // ② 总量必须非负且不小于任一相 ✓。
    let total = timings["total_ms"].as_f64().unwrap();
    for key in PHASES {
        assert!(
            timings[key].as_f64().unwrap() <= total + 1e-6,
            "单项 {key} 不该超过总时长：{timings}"
        );
    }
    // ③ **账目要对得上** ✓：五项（含残差）之和 == 总时长 ✓（容差 0.01 ms ✓，纯浮点/取整误差 ✓）。
    let sum = sum_of_phases(timings);
    assert!(
        (sum - total).abs() < 0.01,
        "各相之和（含残差）必须等于总时长：sum={sum} total={total} {timings}"
    );
    // ④ **真的量到了东西** ✓：一次落笔必然有光栅工作与提交工作 ⇒
    //    这两相里至少一相 > 0 ✓（若全是 0，说明插桩点根本没被走到 ✓）。
    let measured = timings["raster_ms"].as_f64().unwrap() + timings["fold_ms"].as_f64().unwrap();
    assert!(
        measured > 0.0,
        "落笔至少要有光栅或折叠耗时（否则插桩没生效）：{timings}"
    );
    let _ = std::fs::remove_dir_all(&root);

    // ⑤ **没有改动既有字段** ✓（客户端依赖它们 ✓）。
    for key in ["atom_id", "seq", "changeset_id", "head", "dirty_bbox"] {
        assert!(made.get(key).is_some(), "既有字段 {key} 不该消失：{made}");
    }
}

// ---------------------------------------------------------------------------
// P1：在飞拒绝
// ---------------------------------------------------------------------------

/// **判据（P1）**：同文档已有在飞变更操作时，**第二个变更请求被拒绝**（`busy` ✓），
/// 而且**没有被执行** ✓（图层没有真的建出来 ✓）；**只读工具照常** ✓。
///
/// **怎么变红** ✗：
/// * 把 `ToolRegistry::call` 里的 `begin` 去掉 ⇒ 第二次调用返回 `ok:true` ⇒ 第二条断言失败 ✓；
/// * 把忙判定写成"别的文档"（例如 `doc_id != ctx.doc_id` ✓）⇒ 同样变红 ✓；
/// * 把只读工具也拦掉 ⇒ 最后一条断言失败 ✓。
#[test]
fn a_second_mutating_request_is_rejected_while_the_same_document_is_busy() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let doc = "doc_busy";
    workspace
        .create_document(NewDocument::new(doc, 64, 48), "human:1", "session:test")
        .unwrap();

    // **模拟"另一次请求正在跑"** ✓：用**同一张登记表**（生产路径用的是它 ✓）。
    let registry_handle = workspace.inflight().clone();
    let guard = registry_handle
        .begin(doc, "brush_stroke", "human:2", "session:other", 1_000)
        .expect("空登记表应当能登记");

    // ① 变更请求 ⇒ `busy` ✓，并带上"谁在跑、跑了多久" ✓。
    let rejected = call_on(
        &mut workspace,
        doc,
        "create_layer",
        json!({"layer_id": "L_busy"}),
    );
    assert_eq!(rejected["ok"], json!(false), "{rejected}");
    assert_eq!(rejected["error_code"], json!("busy"), "{rejected}");
    assert_eq!(
        rejected["retryable"],
        json!(false),
        "busy 不该被当成可盲目重试：{rejected}"
    );
    // 5.7 的结构是 `{context: {...}}` ✓，结构化补充在 `context.extra` 里 ✓（与既有错误同形 ✓）。
    assert_eq!(
        rejected["context"]["extra"]["status"],
        json!("busy"),
        "{rejected}"
    );
    assert_eq!(
        rejected["context"]["extra"]["inflight"]["tool"],
        json!("brush_stroke"),
        "busy 必须说清是哪个操作：{rejected}"
    );
    assert!(
        rejected["context"]["extra"]["inflight"]["running_ms"]
            .as_u64()
            .is_some(),
        "busy 必须给出已运行时长（这是'慢'与'挂死'的唯一区分依据）：{rejected}"
    );

    // ② **它没有被执行** ✓：图层不存在 ✓。
    let layers = call_on(&mut workspace, doc, "list_layers", json!({}));
    let names = layers["layers"].to_string();
    assert!(
        !names.contains("L_busy"),
        "被 busy 拒绝的请求**绝不能**已经改过文档：{layers}"
    );

    // ③ 只读工具不受影响 ✓（排查"卡在哪"时还得能读 ✓）。
    let read = call_on(&mut workspace, doc, "get_state", json!({}));
    assert_eq!(read["ok"], json!(true), "只读工具不该被 busy 拦住：{read}");

    // ④ 操作结束后 ⇒ 立刻放行 ✓（不是永久锁死 ✓）。
    drop(guard);
    let allowed = call_on(
        &mut workspace,
        doc,
        "create_layer",
        json!({"layer_id": "L_ok"}),
    );
    assert_eq!(allowed["ok"], json!(true), "{allowed}");
}

/// **判据（P1）**：`get_inflight` 能看到在飞操作 ✓，`cancel_operation` 置位取消标志 ✓。
///
/// **怎么变红** ✗：让 `cancel_operation` 只回 `cancelled:true` 而不真的置位 ✓
///（例如误用 `info()` 而不调用 `request_cancel()` ✓）⇒ 最后一条断言失败 ✓。
#[test]
fn inflight_state_is_observable_and_cancel_is_actually_requested() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let doc = "doc_observe";
    workspace
        .create_document(NewDocument::new(doc, 64, 48), "human:1", "session:test")
        .unwrap();

    let registry_handle = workspace.inflight().clone();
    let guard = registry_handle
        .begin(doc, "batch", "human:2", "session:other", 2_000)
        .expect("空登记表应当能登记");

    let status = call_on(&mut workspace, doc, "get_inflight", json!({}));
    assert_eq!(status["ok"], json!(true), "{status}");
    assert_eq!(status["count"], json!(1), "{status}");
    assert_eq!(status["inflight"][0]["tool"], json!("batch"), "{status}");
    assert_eq!(
        status["inflight"][0]["cancel_requested"],
        json!(false),
        "{status}"
    );

    let cancelled = call_on(&mut workspace, doc, "cancel_operation", json!({}));
    assert_eq!(cancelled["ok"], json!(true), "{cancelled}");
    assert_eq!(cancelled["cancelled"], json!(true), "{cancelled}");
    // **关键断言** ✓：标志真的置位了 ✓（不是只回了一句"已取消" ✗）。
    assert!(
        guard.op().cancel_requested(),
        "cancel_operation 必须真的置位取消标志：{cancelled}"
    );

    let status = call_on(&mut workspace, doc, "get_inflight", json!({}));
    assert_eq!(
        status["inflight"][0]["cancel_requested"],
        json!(true),
        "取消之后必须观察得到：{status}"
    );
    assert!(
        status["cancel_requests"].as_u64().unwrap() >= 1,
        "取消请求应当被计数：{status}"
    );
}

/// **判据（P1）**：`cancel_operation` 是控制面动作 ⇒ **viewer 不许打断别人** ✓。
///
/// **怎么变红** ✗：去掉实现里的 `role.can_edit()` 检查 ⇒ 返回 `ok:true` ⇒ 失败 ✓。
#[test]
fn a_viewer_cannot_cancel_someone_elses_operation() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let doc = "doc_viewer_cancel";
    workspace
        .create_document(NewDocument::new(doc, 64, 48), "human:1", "session:test")
        .unwrap();
    let refused = call_on_with(
        &mut workspace,
        doc,
        "cancel_operation",
        json!({}),
        true,
        Role::Viewer,
    );
    assert_eq!(refused["ok"], json!(false), "{refused}");
    assert_eq!(
        refused["error_code"],
        json!("permission_denied"),
        "{refused}"
    );
}

/// **判据（P1）**：取消标志被置位后，`batch` 在**下一个安全点**停手，
/// 并把它**已经落下的那一批整体回滚** ✓ —— 不留"一半的批次" ✗。
///
/// 批次是：`create_layer(L_first)` ⇒ `cancel_operation`（自己取消自己 ✓，确定性 ✓）
/// ⇒ `create_layer(L_second)`。第三个子调用之前必须停 ✓，第一个必须被撤销 ✓。
///
/// **怎么变红** ✗：去掉 `write_batch` 循环顶部的 `ctx.check_cancelled()` ⇒
/// 第二个图层真的会被建出来、响应 `ok:true` ⇒ 前两条断言失败 ✓。
#[test]
fn a_cancelled_batch_stops_and_rolls_its_changeset_back() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let doc = "doc_cancel_batch";
    workspace
        .create_document(NewDocument::new(doc, 64, 48), "human:1", "session:test")
        .unwrap();

    let response = call_on(
        &mut workspace,
        doc,
        "batch",
        json!({"calls": [
            {"tool": "create_layer", "arguments": {"layer_id": "L_first"}},
            {"tool": "cancel_operation", "arguments": {}},
            {"tool": "create_layer", "arguments": {"layer_id": "L_second"}}
        ]}),
    );
    assert_eq!(response["ok"], json!(false), "{response}");
    assert_eq!(response["error_code"], json!("cancelled"), "{response}");
    assert_eq!(
        response["context"]["extra"]["status"],
        json!("cancelled"),
        "{response}"
    );
    // **整批回滚** ✓：第一个子调用落下的图层被撤回 ✓、第三个根本没跑 ✓。
    // `completed_calls` 是"返回过的子调用数" ✓（`cancel_operation` 自己返回了 ✓）⇒ 2 < 3 ✓。
    assert_eq!(response["rolled_back"]["reverted"], json!(1), "{response}");
    assert_eq!(
        response["rolled_back"]["completed_calls"],
        json!(2),
        "{response}"
    );
    assert_eq!(
        response["rolled_back"]["total_calls"],
        json!(3),
        "{response}"
    );
    assert_eq!(response["rolled_back"]["ok"], json!(true), "{response}");

    let layers = call_on(&mut workspace, doc, "list_layers", json!({}));
    let names = layers["layers"].to_string();
    assert!(
        !names.contains("L_first"),
        "取消后第一个子调用的落子必须被撤销：{layers}"
    );
    assert!(
        !names.contains("L_second"),
        "取消后不该再执行后面的子调用：{layers}"
    );
}

// ---------------------------------------------------------------------------
// P1 相关：同步 vs 推迟
// ---------------------------------------------------------------------------

/// **判据（P1 相关）**：`batch` / `scatter_strokes` 必须说清自己是
/// **同步做完**还是**把活留到了下一次渲染** ✓。
///
/// **怎么变红** ✗：把 `execution` 字段去掉 ⇒ 第一条断言失败 ✓；
/// 或永远报 `"synchronous"` ⇒ 推迟那一档失败 ✓。
#[test]
fn a_deferred_batch_says_so_instead_of_pretending_it_finished() {
    let root = temp_dir("deferred");
    let mut workspace = brush_workspace(&root, "doc_deferred");
    // **两台文档** ✓：job 队列是**按文档**的 ✓ ⇒ 推迟那一档遗留的 pending job
    // 不会污染同步那一档的判定 ✓（否则测的就不是 `wait_for_render` 而是"前面的残留"✗）。
    workspace
        .create_document(
            NewDocument::new("doc_deferred_sync", 320, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    for doc in ["doc_deferred", "doc_deferred_sync"] {
        assert_eq!(
            call_on(
                &mut workspace,
                doc,
                "create_layer",
                json!({"layer_id": "L"})
            )["ok"],
            json!(true)
        );
    }
    let batch_of = |object_id: &str| {
        json!({
            "silent": true,
            "calls": [
                {"tool": "brush_stroke", "arguments": {
                    "layer_id": "L", "object_id": object_id, "brush": "2B_pencil", "size": 16,
                    "points": [[20.0, 40.0, 0.5], [120.0, 90.0, 0.8]]}}
            ]
        })
    };

    // ① **推迟**：`wait_for_render=false` ⇒ 重型原子的渲染 job 留在队列里 ✓。
    let deferred = call_on_with(
        &mut workspace,
        "doc_deferred",
        "batch",
        batch_of("d1"),
        false,
        Role::Owner,
    );
    assert_eq!(deferred["ok"], json!(true), "{deferred}");
    assert_eq!(
        deferred["execution"]["mode"],
        json!("deferred"),
        "推迟时必须如实说推迟：{deferred}"
    );
    assert_eq!(deferred["execution"]["deferred"], json!(true), "{deferred}");
    assert!(
        deferred["execution"]["pending_jobs"].as_u64().unwrap() >= 1,
        "推迟时应当有未完成的渲染 job：{deferred}"
    );

    // ② **同步**：默认 `wait_for_render=true` ⇒ job 在本次调用内跑完 ✓。
    let synchronous = call_on_with(
        &mut workspace,
        "doc_deferred_sync",
        "batch",
        batch_of("d2"),
        true,
        Role::Owner,
    );
    assert_eq!(synchronous["ok"], json!(true), "{synchronous}");
    assert_eq!(
        synchronous["execution"]["mode"],
        json!("synchronous"),
        "{synchronous}"
    );
    assert_eq!(
        synchronous["execution"]["pending_jobs"],
        json!(0),
        "{synchronous}"
    );

    // ③ **每个子调用各自带真实耗时** ✓（报告说"batch 里每笔的数字是假的"✗）。
    let first = &synchronous["calls"][0]["result"];
    assert!(
        first["timings"]["total_ms"].as_f64().is_some(),
        "batch 子调用必须带自己的 timings：{first}"
    );
    assert!(
        first["timings"]["raster_ms"].as_f64().unwrap() > 0.0,
        "子调用的光栅耗时必须真的被量到：{first}"
    );

    // ④ `scatter_strokes` 同样要说 ✓。
    let scattered = call_on_with(
        &mut workspace,
        "doc_deferred",
        "scatter_strokes",
        json!({
            "layer_id": "L", "brush": "2B_pencil", "seed": 7,
            "area": {"x": 10.0, "y": 10.0, "w": 60.0, "h": 40.0},
            "count": 2,
            "palette": ["#102030"]
        }),
        false,
        Role::Owner,
    );
    assert_eq!(scattered["ok"], json!(true), "{scattered}");
    assert_eq!(
        scattered["execution"]["deferred"],
        json!(true),
        "{scattered}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
