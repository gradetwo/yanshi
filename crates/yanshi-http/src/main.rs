//! `yanshi-serve` 可执行入口：零依赖 HTTP + WebSocket 服务端。

use yanshi_http::{serve, HttpOptions};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // **`--version` 要与健康接口说同一句话** ✓（用户提的排查建议 ✓）：
    // 拿到一个正在跑的服务时 ✓，第一句想问的往往是"这是哪个 commit 的构建" ✓ ——
    // `--version` 与 `/api/health` 用**同一个** `build_identity()` ✓ ⇒ 两处永远不会互相矛盾 ✓。
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("{}", yanshi_http::server::build_identity());
        return;
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("{}", HttpOptions::help());
        return;
    }
    let options = match HttpOptions::parse_args(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("参数错误：{error}\n\n{}", HttpOptions::help());
            std::process::exit(2);
        }
    };
    match serve(options) {
        Ok(handle) => {
            println!("偃师 Yanshi 服务已启动：{}", handle.base_url());
            println!(
                "打开 {} 使用最小 Web 查看器（自动创建 capability token）",
                handle.base_url()
            );
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        }
        Err(error) => {
            eprintln!("启动失败：{error}");
            std::process::exit(1);
        }
    }
}
