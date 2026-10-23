//! **读侧的游标与整数参数** —— 三条「声明了却没生效」的病。
//!
//! 三条断言各自对准一类「看起来对、其实错了」✓：
//! ① `list_comments` 的 `next_since` 取的是**匹配全集**的最大 seq，而交付的却是**最新一页** ✗
//!    ⇒ 比这一页更早的那些评论**永远不会再被返回**（客户端照着游标轮询 ⇒ 静默丢消息 ✓）；
//! ② `find_atom` 把 `limit` 悄悄夹到 2000，回执里**既没有 `limit`、也没有游标** ✗
//!    ⇒ 第 2001 条之后的原子**取不到**，而 `truncated` 把「被你夹了」与「还有下一页」混成一句话 ✓；
//! ③ `validate_args` 对 `Integer` 接受**负数**与**整数形态的浮点**（`-1`／`5.0` ✓），
//!    而处理侧统一用 `Value::as_u64` 取数 ⇒ 这两类值**变成 `None`**，
//!    于是**静默回落到缺省**（`since_seq:-1` ⇒ 当成 0 ⇒ 返回全量 ✓）——
//!    本仓把它列为静默降级形态 4（非法输入看起来正常 ✓）。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_cursor", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn workspace_with_document() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_cursor", 32, 32),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

/// 写 `n` 条评论，返回它们的正文（按写入顺序 ✓）。
fn post_comments(workspace: &mut Workspace, n: usize) -> Vec<String> {
    let mut texts = Vec::new();
    for index in 0..n {
        let text = format!("第 {index} 条");
        let mut ctx = context(workspace);
        let posted = registry().call(&mut ctx, "comment", &json!({ "text": text.clone() }));
        assert_eq!(posted["ok"], json!(true), "{posted}");
        texts.push(text);
    }
    texts
}

fn array(value: &Value, key: &str) -> Vec<Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("响应里应当有 {key} 数组：{value}"))
        .clone()
}

/// ① **照着 `next_since` 轮询，一条都不能丢** ✓。
///
/// **输入制造出它要检查的情况** ✓：5 条评论、`limit:2` ⇒ 必然截断 ✓
/// （原来只有 2 条被交付、游标却跳到全局最大 ⇒ 其余 3 条永久消失 ✗）。
#[test]
fn polling_the_comment_cursor_delivers_every_comment() {
    let mut workspace = workspace_with_document();
    let texts = post_comments(&mut workspace, 5);
    let mut since = 0_u64;
    let mut received = Vec::new();
    for round in 0..10 {
        let mut ctx = context(&mut workspace);
        let page = registry().call(
            &mut ctx,
            "list_comments",
            &json!({ "since_seq": since, "limit": 2 }),
        );
        assert_eq!(page["ok"], json!(true), "{page}");
        let items = array(&page, "comments");
        if items.is_empty() {
            break;
        }
        // 页内**最新的在前**是既有语义 ⇒ 不能改（改了会骗过老客户端 ✓）。
        for item in &items {
            received.push(item["text"].as_str().expect("评论应有正文").to_owned());
        }
        since = page["next_since"]
            .as_u64()
            .unwrap_or_else(|| panic!("第 {round} 轮没有 next_since：{page}"));
    }
    received.sort();
    let mut expected = texts.clone();
    expected.sort();
    assert_eq!(
        received, expected,
        "照游标轮询必须收齐 5 条（丢的那几条没有任何信号）"
    );
}

/// ①′ **满页要说「还有」** ✓：`truncated` 与「交付数 vs 匹配数」的关系必须自洽。
#[test]
fn a_full_page_of_comments_says_there_are_more() {
    let mut workspace = workspace_with_document();
    post_comments(&mut workspace, 5);
    let mut ctx = context(&mut workspace);
    let page = registry().call(&mut ctx, "list_comments", &json!({ "limit": 2 }));
    let items = array(&page, "comments");
    let count = page["count"]
        .as_u64()
        .expect("count 是匹配总数（既有语义，不改 ✓）");
    // **恒等式**（优于单点断言 ✓）：满页 ⇔ 还有更多。
    assert_eq!(items.len() as u64, 2, "limit=2 ⇒ 本页恰好 2 条：{page}");
    assert_eq!(
        page["truncated"].as_bool(),
        Some(count > items.len() as u64),
        "truncated 必须等于「匹配总数 > 本页交付数」：{page}"
    );
    assert_eq!(
        page["returned"].as_u64(),
        Some(items.len() as u64),
        "{page}"
    );
    // 游标只能停在**已交付**的那几条里 ⇒ 否则未交付的就被跳过 ✗。
    let delivered_max = items
        .iter()
        .filter_map(|item| item["seq"].as_u64())
        .max()
        .expect("页内应有 seq");
    assert_eq!(
        page["next_since"].as_u64(),
        Some(delivered_max),
        "游标不得超过已交付的最大 seq（超了就永久丢）：{page}"
    );
}

/// ② `find_atom` 必须**回显它真正用的 limit**，并且说清是被上限夹的 ✓。
#[test]
fn find_atom_echoes_the_limit_it_applied() {
    let mut workspace = workspace_with_document();
    post_comments(&mut workspace, 3);
    let mut ctx = context(&mut workspace);
    let response = registry().call(&mut ctx, "find_atom", &json!({ "limit": 5000 }));
    assert_eq!(response["ok"], json!(true), "{response}");
    assert_eq!(
        response["limit"].as_u64(),
        Some(2000),
        "上限 2000 要回显（原来只改内部值、回执里没有 limit ⇒ 调用方以为要到了 5000 ✓）：{response}"
    );
    assert_eq!(
        response["limit_requested"].as_u64(),
        Some(5000),
        "被夹过时要给出请求值：{response}"
    );
    assert_eq!(
        response["limit_clamped"].as_bool(),
        Some(true),
        "「被你夹了」必须是独立字段，不能混进 truncated：{response}"
    );
}

/// ②′ `find_atom` 必须给出**游标**，否则第 2000 条之后永远取不到 ✓。
#[test]
fn find_atom_returns_a_cursor_so_the_rest_is_reachable() {
    let mut workspace = workspace_with_document();
    post_comments(&mut workspace, 4);
    let mut ctx = context(&mut workspace);
    let first = registry().call(&mut ctx, "find_atom", &json!({ "limit": 2 }));
    let atoms = array(&first, "atoms");
    assert_eq!(atoms.len(), 2, "{first}");
    let cursor = first["next_since"]
        .as_u64()
        .unwrap_or_else(|| panic!("截断时必须给游标，否则剩下的取不到：{first}"));
    assert_eq!(
        first["truncated"].as_bool(),
        Some(true),
        "还有匹配没交付 ⇒ truncated 为真：{first}"
    );
    let mut ctx = context(&mut workspace);
    let second = registry().call(
        &mut ctx,
        "find_atom",
        &json!({ "limit": 2, "since_seq": cursor }),
    );
    let more = array(&second, "atoms");
    assert_eq!(more.len(), 2, "照游标应拿到剩下 2 条：{second}");
    let mut seqs: Vec<u64> = atoms
        .iter()
        .chain(more.iter())
        .filter_map(|atom| atom["seq"].as_u64())
        .collect();
    seqs.sort_unstable();
    seqs.dedup();
    assert_eq!(
        seqs.len(),
        4,
        "两页合起来应覆盖 4 条且不重不漏：{first}＋{second}"
    );
}

/// ③ **负数游标不许当成 0** ✓（形态 4：非法输入看起来正常 ✓）。
#[test]
fn a_negative_cursor_is_refused_not_treated_as_zero() {
    let mut workspace = workspace_with_document();
    post_comments(&mut workspace, 3);
    for name in ["find_atom", "list_atoms", "get_stroke_log"] {
        let mut ctx = context(&mut workspace);
        let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            registry().call(&mut ctx, name, &json!({ "since_seq": -1 }))
        }));
        let detail = match response {
            Ok(value) => {
                assert_eq!(
                    value.get("ok").and_then(Value::as_bool),
                    Some(false),
                    "{name} 把 since_seq:-1 静默当 0 处理了 ⇒ 返回全量而没有信号：{value}"
                );
                value
                    .get("context")
                    .and_then(|c| c.get("detail"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            }
            Err(_) => "调用直接失败".to_owned(),
        };
        assert!(
            detail.contains("since_seq"),
            "{name} 的错误必须点名参数（不是只报「参数非法」）：{detail}"
        );
        assert!(
            detail.contains("≥ 0") || detail.contains("不小于 0"),
            "{name} 的错误要说清合法区间：{detail}"
        );
    }
}

/// ③′ **整数形态的浮点仍要生效** ✓：`since_seq:2.0` 与 `since_seq:2` 必须给同一个结果。
#[test]
fn an_integer_written_as_a_float_is_still_honoured() {
    let mut workspace = workspace_with_document();
    post_comments(&mut workspace, 5);
    let mut ctx = context(&mut workspace);
    let all = registry().call(&mut ctx, "find_atom", &json!({}));
    let total = all["total_matched"].as_u64().expect("总数");
    assert!(total >= 5, "至少 5 条评论原子：{all}");
    let cursor = all["atoms"]
        .as_array()
        .and_then(|atoms| atoms.iter().filter_map(|a| a["seq"].as_u64()).min())
        .expect("应有 seq");
    let mut ctx = context(&mut workspace);
    let int_form = registry().call(&mut ctx, "find_atom", &json!({ "since_seq": cursor + 1 }));
    let mut ctx = context(&mut workspace);
    let float_form = registry().call(
        &mut ctx,
        "find_atom",
        &json!({ "since_seq": (cursor + 1) as f64 }),
    );
    assert_eq!(
        int_form["total_matched"], float_form["total_matched"],
        "同一个数写成 3 与 3.0 ⇒ 结果必须一致（原来 3.0 被 as_u64 读成 None ⇒ 当成 0 ⇒ 返回全量 ✓）"
    );
    assert!(
        float_form["total_matched"].as_u64().unwrap_or(total) < total,
        "游标真的生效才叫生效：{float_form}"
    );
}
