//! **`blob_gc` 的内存闸**（外部评测包 `B4`）：宁可**拒绝**，也不许被 OOM 杀掉。
//!
//! **他们报的**：25 499 原子的 4K 文档，`blob_gc` **即使 `dry_run:true`** 也被 SIGKILL
//! （`rc=-9`）⇒ 进程死掉，调用方只看到 "server closed" ✗。
//!
//! **第 1551 轮已加闸**（`ResourceExhausted` ＋ 两个数字），但**没有判据**：
//! 常量写死 1.5 GiB ⇒ 只有真造 1.5 GiB 的文档才碰得到 ⇒ 等于"写了但永远不验" ✗。
//! ⇒ 本轮把上限做成可覆盖的（`YANSHI_GC_MEMORY_BUDGET_MB` ✓），于是能**便宜地**钉住这条：
//!
//! ① 上限调到 1 MB ⇒ `blob_gc` 必须**明确拒绝**（`resource_exhausted`），
//!    文案里要有**估算值**与**原子数**（他们据此分阶段跑 ✓）；
//! ② **进程没死**：紧接着另一个只读工具必须照常返回 ✓（被 OOM 杀时这条拿不到）；
//! ③ 上限调大 ⇒ 同一个调用**照常成功** ✓（证明闸不是"永远拒绝" ✗）。
//!
//! **变异**：把闸去掉（或把上限改回写死的大值）⇒ ① 红（它直接成功了），③ 仍绿。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_gc_budget", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 8_000)
}

fn call(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> Value {
    registry().call(ctx, name, args)
}

#[test]
fn blob_gc_refuses_instead_of_being_killed_when_the_estimate_exceeds_the_budget() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    {
        let mut ctx = context(&mut workspace);
        call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_gc_budget", "width": 64, "height": 64}),
        );
        let layers = call(&mut ctx, "list_layers", &json!({}));
        let layer = layers
            .get("layers")
            .and_then(Value::as_array)
            .and_then(|layers| layers.first())
            .and_then(|layer| layer.get("layer_id").or_else(|| layer.get("id")))
            .and_then(Value::as_str)
            .expect("新文档应有图层")
            .to_string();
        // 攒几个原子：估算按「原子数 × ≈300 KB」算 ⇒ 4 个就超过 1 MB。
        for index in 0..4 {
            call(
                &mut ctx,
                "fill_region",
                &json!({
                    "layer_id": layer,
                    "shape": {"type": "rect", "x": index * 8, "y": 0, "w": 8, "h": 8},
                    "color": [10, 20, 30, 255]
                }),
            );
        }
    }

    let mut ctx = context(&mut workspace);
    // ① 上限 1 MB ⇒ 必须**明确拒绝**（而不是被 OOM 杀）。
    std::env::set_var("YANSHI_GC_MEMORY_BUDGET_MB", "1");
    let refused = call(&mut ctx, "blob_gc", &json!({"dry_run": true}));
    assert_eq!(
        refused.get("ok").and_then(Value::as_bool),
        Some(false),
        "估算超过上限时必须**拒绝**，而不是硬上（外部 B4 就是被 SIGKILL 的）⇒ {refused}"
    );
    assert_eq!(
        refused.get("error_code").and_then(Value::as_str),
        Some("resource_exhausted"),
        "拒绝的理由必须是 resource_exhausted（可重试/可分阶段跑）⇒ {refused}"
    );
    let detail = refused
        .get("context")
        .and_then(|context| context.get("detail"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    assert!(
        detail.contains("MB") && detail.contains("原子"),
        "文案必须给出**估算值**与**原子数**（他们要据此分阶段跑）：{detail}"
    );

    // ② 进程还活着：紧接一个只读工具必须照常返回（被 OOM 杀时根本走不到这里）。
    let alive = call(&mut ctx, "get_log", &json!({}));
    assert_eq!(
        alive.get("ok").and_then(Value::as_bool),
        Some(true),
        "拒绝之后进程必须仍然可用（这正是「拒绝」相对「被杀」的全部意义）：{alive}"
    );

    // ③ 把上限调大 ⇒ 同一个调用必须照常成功（闸不是「永远拒绝」✗）。
    std::env::set_var("YANSHI_GC_MEMORY_BUDGET_MB", "4096");
    let allowed = call(&mut ctx, "blob_gc", &json!({"dry_run": true}));
    assert_eq!(
        allowed.get("ok").and_then(Value::as_bool),
        Some(true),
        "上限抬高之后必须放行（否则这闸就是「一律拒绝」✗）：{allowed}"
    );
    std::env::remove_var("YANSHI_GC_MEMORY_BUDGET_MB");
}
