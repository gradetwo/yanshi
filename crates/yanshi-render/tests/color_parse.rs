/// **颜色对象里的未知键必须被拒绝** ✗ —— 真实 MCP 报告（2026-10-03 ✓）：
/// `gradient_fill{from:{color:"#FF0000"}}` 返回 `ok=true` ✓ 而画出来**全黑** ✗，
/// 因为 `r`/`g`/`b` 全部取了默认 0 ✓、那个多余的键被**静默忽略** ✓。
/// **判据** ✓（能红 ✓）：带未知键 ⇒ 解析必须失败 ✓；正规写法 ⇒ 必须照旧成功 ✓。
#[test]
fn an_unknown_key_in_a_colour_object_is_refused() {
    use serde_json::json;
    assert!(
        yanshi_render::color::parse_spec_color(&json!({"color": "#FF0000"})).is_none(),
        "带未知键的颜色对象必须被拒绝 ✗（以前会静默变成黑色 ✓）"
    );
    assert!(
        yanshi_render::color::parse_spec_color(&json!({"r": 255, "g": 0, "b": 0, "a": 255}))
            .is_some(),
        "正规写法必须照旧可用 ✓"
    );
    assert!(
        yanshi_render::color::parse_spec_color(&json!({"r": 255, "g": 0, "b": 0, "alpha": 255}))
            .is_none(),
        "`alpha` 不是我们的键（我们叫 `a` ✓）⇒ 也要拒绝 ✓，不许静默忽略 ✗"
    );
}
