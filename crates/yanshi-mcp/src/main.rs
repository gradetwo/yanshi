//! `yanshi-mcp` 可执行入口：把 stdin/stdout 接成 MCP stdio 服务器。

use std::io::{self, Write};
use yanshi_mcp::{serve, McpOptions, SERVER_NAME, SERVER_VERSION};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("{}", McpOptions::help());
        return;
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        // **带上 commit 与构建时间** ✓（用户提的排查建议 ✓）：与 `yanshi-serve` 同一套信息 ✓。
        println!(
            "{SERVER_NAME} {SERVER_VERSION} (commit {}, built {})",
            option_env!("YANSHI_COMMIT").unwrap_or("unknown"),
            option_env!("YANSHI_BUILD_TIME").unwrap_or("unknown")
        );
        return;
    }
    if args.iter().any(|arg| arg == "--list-tools") {
        let options = match McpOptions::parse_args(args) {
            Ok(options) => options,
            Err(error) => {
                eprintln!("参数错误：{error}\n\n{}", McpOptions::help());
                std::process::exit(2);
            }
        };
        let server = yanshi_mcp::Server::new(options);
        println!("{}", server.tools_list());
        return;
    }

    let options = match McpOptions::parse_args(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("参数错误：{error}\n\n{}", McpOptions::help());
            std::process::exit(2);
        }
    };

    let stdin = io::stdin();
    let stdout = io::stdout();
    if let Err(error) = serve(stdin.lock(), stdout.lock(), options) {
        let _ = writeln!(io::stderr(), "{SERVER_NAME}: {error}");
        std::process::exit(1);
    }
}
