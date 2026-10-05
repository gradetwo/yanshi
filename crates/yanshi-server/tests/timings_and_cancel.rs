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
///
/// **`render_ms` / `png_ms` 是导出路径的两相** ✓（其余工具恒为 0 ✓）；它们**必须在这里** ✓ ——
/// `other_ms` 是"总时长 − 已量各相" ✓ ⇒ 若把新两相漏在这张表外 ✗，
/// `sum_of_phases` 就会比 `total_ms` **少掉这两相** ✓ ⇒ ③ 的和式判据当场变红 ✓
///（这正是"新相加进来、残差要相应缩小"这条约束的判据形式 ✓）。
const PHASES: [&str; 7] = [
    "prep_ms",
    "raster_ms",
    "dirty_ms",
    "fold_ms",
    "log_ms",
    "render_ms",
    "png_ms",
];

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

/// **已量各相之和** ✓（不含残差 ✓）—— 判据 ③ 要看的就是"残差之外到底量到了多少" ✓。
fn sum_of_measured_phases(timings: &Value) -> f64 {
    PHASES
        .iter()
        .map(|key| {
            timings[*key]
                .as_f64()
                .unwrap_or_else(|| panic!("timings.{key} 必须是数字：{timings}"))
        })
        .sum::<f64>()
}

// ---------------------------------------------------------------------------
// P2：阶段耗时
// ---------------------------------------------------------------------------

/// **判据（P2）**：写路径响应带 `timings` ✓，且**各相之和 == 报告的总时长** ✓。
///
/// **怎么变红** ✗（两条都实测过 ✓）：
/// * 去掉附 `timings` 的那两处（`dispatch` 与 `ToolRegistry::call` ✓）⇒ 键缺失 ⇒ 第一条断言失败 ✓；
/// * 把残差算错（`other_ms` 直接用 `total` 而不是 `total - 已量各相` ✓）⇒
///   各相之和**大于** `total_ms` ⇒ 第三条断言失败 ✓（实测 sum=2031.8 vs total=1897.7 ✓）。
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

// ---------------------------------------------------------------------------
// P2 续：导出路径的两相（render_ms / png_ms）
// ---------------------------------------------------------------------------

/// **判据（导出两相，② + ③）**：一次真实 `export_png` 的报告里
/// `render_ms` 与 `png_ms` **真的被量到** ✓（不是只多两个恒为 0 的字段 ✗），
/// 而且**残差不该吞掉几乎全部导出时间** ✓。
///
/// **为什么需要它** ✓（本轮开工时的实测 ✓）：4K / debug 导出（`/tmp/cold4k`、`parrot-4k-bold` ✓）
/// 的读数是 `total_ms=246176.9` 而 **`other_ms=246176.9`** ✓ —— 渲染与编码全被残差吞掉 ✗
/// ⇒ 一条读数分不清"慢在渲染"还是"慢在编码" ✗。本判据要能当场抓住这种读数 ✓。
///
/// **阈值从哪来** ✓（不是拍的 ✓）：同一台机器上，本测试这份 **800×600 / debug / 内存工作区**
/// 的导出实测（`--nocapture` 打印在下面 ✓）：
/// `total_ms=1027.474`、`render_ms=755.328`、`png_ms=271.845`、`other_ms=0.294`
/// ⇒ `render+png` 占 **99.97%** ✓、残差占 **0.03%** ✓；而**旧代码**下这两相是 0 ✗
///（占 0% ✓、残差占 100% ✗）。
/// ⇒ 阈值取 **0.5** 是"比实测低 ~50 个百分点"的**保守线** ✓：
/// 它抓得住"两相没被量到"（0% ✗）与"两相被别的开销盖过一半以上"（<50% ✗）✓，
/// 抓不住"渲染与编码**之间**的比例是否合理" ✓（那不在本判据的目的内 ✓，
/// 比例要由读数的人按 4K 实测判断 ✓）。
///
/// **怎么变红** ✗（两条都实测过 ✓，且是**直接**跑本测试文件 ✓）：
/// * 去掉 `write_export_png` 里的 `ctx.time(Phase::Render, …)` 与 `Phase::Png` ⇒
///   两相为 0.0 ⇒ ①②③ 全红 ✓（旧读数 `other_ms == total_ms` 正是这一条要抓的 ✓）；
/// * 把 `report()` 里的 `other_us` 写回 `total_us`（残差不减已量各相 ✓）⇒
///   各相之和大于总时长 ⇒ 既有判据红 ✓ **且**本判据的 ③ 红 ✓
///   （实测 `other` 占比 = 1，而 `render_ms/png_ms` 仍非零 ⇒ ② 反而绿 ✓ ——
///   这正是判据 ③ 相对判据 ② 的独立覆盖：盯住残差字段本身算错 ✓）。
#[test]
fn an_export_reports_render_and_encode_phases_and_a_small_residual() {
    // **本判据的门槛** ✓：两相合计至少占总时长的这个比例 ✓（推导见 doc comment ✓）。
    const MIN_RENDER_PLUS_PNG_SHARE: f64 = 0.5;
    // **残差上限** ✓：残差最多占总时长的这个比例 ✓（即已量各相至少一半 ✓）。
    const MAX_OTHER_SHARE: f64 = 0.5;

    let doc = "doc_export_timings";
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(NewDocument::new(doc, 800, 600), "human:1", "session:test")
        .unwrap();
    let path =
        std::env::temp_dir().join(format!("yanshi_timing_export_{}.png", std::process::id()));
    let _ = std::fs::remove_file(&path);

    let exported = {
        let mut ctx = ToolContext::new(&mut workspace, doc, "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        let registry = registry();
        assert_eq!(
            registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}))["ok"],
            json!(true)
        );
        assert_eq!(
            registry.call(
                &mut ctx,
                "draw_shape",
                &json!({"layer_id": "L", "object_id": "sq",
                        "data": {"geometry": {"kind": "rect",
                                              "bbox": {"x": 40, "y": 40, "w": 300, "h": 200}},
                                 "color": {"r": 220, "g": 60, "b": 40, "a": 255}}})
            )["ok"],
            json!(true)
        );
        // **每一次 `call` 都会重置并重新累计 timings** ✓ ⇒ 这里读到的是导出这一次的 ✓。
        registry.call(
            &mut ctx,
            "export_png",
            &json!({ "path": path.to_string_lossy() }),
        )
    };
    assert_eq!(exported["ok"], json!(true), "{exported}");

    let timings = exported
        .get("timings")
        .unwrap_or_else(|| panic!("导出响应必须带 timings：{exported}"));
    let total = timings["total_ms"].as_f64().expect("total_ms 是数字");
    let render = timings["render_ms"].as_f64().expect("render_ms 是数字");
    let png = timings["png_ms"].as_f64().expect("png_ms 是数字");
    let other = timings["other_ms"].as_f64().expect("other_ms 是数字");
    eprintln!(
        "MEASURED-EXPORT-INMEMORY-800x600-DEBUG: {timings}  render+png share={:.4}",
        (render + png) / total
    );

    // ① **两相都真的被量到** ✓：存在 ≠ 已量 ✓（旧代码这里是 0.0 ✗）。
    assert!(render > 0.0, "导出的渲染相必须真的被量到：{timings}");
    assert!(png > 0.0, "导出的编码相必须真的被量到：{timings}");

    // ② **两相合计是总时长的实质一部分** ✓（不是被残差盖过的零头 ✓）。
    let render_png_share = (render + png) / total;
    assert!(
        render_png_share >= MIN_RENDER_PLUS_PNG_SHARE,
        "渲染＋编码至少应占总时长的 {MIN_RENDER_PLUS_PNG_SHARE}，实测 {render_png_share}：{timings}"
    );

    // ③ **残差没有吞掉几乎全部导出** ✓：已量各相至少占一半 ✓。
    //    旧读数 `other_ms == total_ms`（other 占比 100% ✗）在这里当场红 ✓。
    let other_share = other / total;
    assert!(
        other_share <= MAX_OTHER_SHARE,
        "残差最多应占总时长的 {MAX_OTHER_SHARE}（已量各相至少一半），实测 other 占比 {other_share}：{timings}"
    );
    assert!(
        sum_of_measured_phases(timings) >= total * (1.0 - MAX_OTHER_SHARE),
        "已量各相之和至少应占总时长的一半：{timings}"
    );

    // ④ **账目仍然对得上** ✓（与既有判据同式 ✓，在新两相存在的前提下再验一次 ✓）。
    let sum = sum_of_phases(timings);
    assert!(
        (sum - total).abs() < 0.01,
        "各相之和（含残差）必须等于总时长：sum={sum} total={total} {timings}"
    );
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// 手动探针：真实 4K 导出的读数
// ---------------------------------------------------------------------------

/// **真实 4K 导出的读数探针**（**默认 ignore** ✓，只在有真实工程时手动跑 ✓）。
///
/// 与 `coldstart_measure.rs` 同规矩 ✓：**只打印读数、不设性能门槛** ✓
///（数字随机器与构建档变化 ✓ ⇒ 不做断言 ✓，只断言调用成功 ✓）。
///
/// 用法：
/// ```text
/// YANSHI_COLD_DOC_ROOT=/tmp/cold4k YANSHI_COLD_DOC_ID=parrot-4k-bold \
///   cargo test -p yanshi-server --test timings_and_cancel \
///     measure_real_4k_export_timings -- --ignored --nocapture
/// ```
///
/// **为什么留这枚探针** ✓：本轮的验收读数（4K / debug / 3840×2160 ✓）就是它跑出来的 ✓
/// ⇒ 下次有人报"导出慢"时可以直接重跑同一份读数 ✓，而不必再从外部反推 ✓。
#[test]
#[ignore = "需要真实 4K 工程（约 4 分钟 / debug），手动运行"]
fn measure_real_4k_export_timings() {
    let root = std::env::var("YANSHI_COLD_DOC_ROOT").unwrap_or_else(|_| "/tmp/cold4k".to_owned());
    let doc_id =
        std::env::var("YANSHI_COLD_DOC_ID").unwrap_or_else(|_| "parrot-4k-bold".to_owned());
    // **没有工程就跳过** ✓（CI 上没有 /tmp/cold4k ✓，探针不该因此变红 ✓）。
    let store = match yanshi_server::persist::FileStore::open(&root) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("跳过：文件存储 {root} 打不开（{error}）");
            return;
        }
    };
    if store
        .load_atoms(&doc_id)
        .map(|atoms| atoms.is_empty())
        .unwrap_or(true)
    {
        eprintln!("跳过：{root} 里没有文档 {doc_id}");
        return;
    }

    let path = std::env::temp_dir().join(format!("yanshi_4k_export_{}.png", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let started = std::time::Instant::now();
    let mut workspace =
        Workspace::with_file_store(&root, DocumentSettings::default()).expect("打开落盘工作区");
    workspace.open_document(&doc_id).expect("打开 4K 文档");
    let exported = {
        let mut ctx = ToolContext::new(&mut workspace, &doc_id, "human:1", "session:measure")
            .with_owner(true)
            // **渲染预算放宽到 10 分钟** ✓：debug 下 4K 整幅渲染本身就要几分钟 ✓。
            .with_wait_for_render(true, 600_000);
        registry().call(
            &mut ctx,
            "export_png",
            &json!({ "path": path.to_string_lossy() }),
        )
    };
    let wall = started.elapsed();
    eprintln!(
        "4K-EXPORT wall={wall:?} ok={} width={} height={} bytes={}",
        exported["ok"], exported["width"], exported["height"], exported["bytes"]
    );
    eprintln!("4K-EXPORT timings={}", exported["timings"]);
    assert_eq!(exported["ok"], json!(true), "{exported}");
    // ① 两相必须真的被量到 ✓（这条是探针里唯一的存在性断言 ✓，不是性能门槛 ✓）。
    let timings = &exported["timings"];
    assert!(
        timings["render_ms"].as_f64().unwrap_or(0.0) > 0.0,
        "4K 导出的渲染相必须被量到：{exported}"
    );
    assert!(
        timings["png_ms"].as_f64().unwrap_or(0.0) > 0.0,
        "4K 导出的编码相必须被量到：{exported}"
    );
    let _ = std::fs::remove_file(&path);
}
