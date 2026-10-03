//! **把 `scripts/target-installed-check.sh` 接进测试套** ✓（(B)③ 的收尾 ✓）。
//!
//! 为什么要有它 ✓：那条判据上一轮只作为**独立脚本**存在 ✗ ⇒ `cargo test` **不会**跑它 ✗
//! ⇒ 等于"**判据在、但没人看着**" ✗ ⇒ 这里把它挂进套件 ✓ ⇒ 每次 `cargo test` 都会执行 ✓。
//!
//! 脚本自己**能红** ✓（上一轮已用**变异**证明 ✓：把 `return 2` 改回 `return 0` ✗ ⇒ 脚本退出 1 ✓），
//! 所以这条测试也不需要另造输入 ✓ —— 它只是"**别让它被忘记**" ✓。
use std::process::Command;

#[test]
fn the_installed_target_criterion_passes_in_this_checkout() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("仓库根应当存在");
    let script = root.join("scripts/target-installed-check.sh");
    assert!(script.is_file(), "判据脚本应当存在：{}", script.display());
    let output = Command::new("bash")
        .arg(&script)
        .current_dir(root)
        .output()
        .expect("应当能运行 bash");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "三态判据必须通过 ⇒ stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    // **顺带钉住"它确实检查了三件事"** ✓，免得脚本被改成空跑也仍然"绿" ✗。
    let ok_lines = stdout
        .lines()
        .filter(|line| line.trim_start().starts_with("ok "))
        .count();
    assert_eq!(ok_lines, 3, "应当有三条通过的断言 ⇒ 实际 output:\n{stdout}");
}
