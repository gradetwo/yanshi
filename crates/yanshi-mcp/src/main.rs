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
        println!("{SERVER_NAME} {SERVER_VERSION}");
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
