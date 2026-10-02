//! **`--help` 必须列全可用的 profile** ✓（真实用户 §五-19 ✓）。
//!
//! **原症状** ✓：`--help` 只写 `core,history,annotation,...` ✗ ⇒ 省略号后面还有什么**不知道** ✗。
//! **为什么用测试守住** ✓：`help()` 是 `const fn` ✓ ⇒ 列表只能**静态**写在字符串里 ✗
//! ⇒ 迟早会与 `Profile::ALL` 漂移 ✗ ⇒ 那就**让测试红** ✓，而不是让文档悄悄过时 ✓
//!（这正是用户此前"以为介质还没做"的同一类问题 ✓）。

use yanshi_server::Profile;

/// **每一个 profile 都要在 `--help` 里出现** ✓。
#[test]
fn the_help_lists_every_profile() {
    let help = yanshi_http::server::HttpOptions::help();
    let mut missing = Vec::new();
    for profile in Profile::ALL {
        let name = profile.as_str();
        if !help.contains(name) {
            missing.push(name);
        }
    }
    assert!(
        missing.is_empty(),
        "这些 profile 没在 --help 里列出来：{missing:?}\n--- help ---\n{help}"
    );
    // **且要说明缺省行为** ✓：用户最想问的正是"默认开了哪些" ✓。
    assert!(help.contains("缺省"), "应当说明缺省启用哪些：{help}");
    assert!(
        help.contains("semantic"),
        "应当点名按裁定只预留不开发的那一组：{help}"
    );
}
