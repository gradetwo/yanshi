//! `yanshi-serve` 可执行入口：零依赖 HTTP + WebSocket 服务端。

use yanshi_http::{serve, HttpOptions};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
