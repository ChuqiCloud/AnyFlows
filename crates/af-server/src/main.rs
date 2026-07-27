//! AnyFlows 服务进程入口。

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    af_server::install_process_safety_hooks();
    let Err(error) = af_server::run().await else {
        return ExitCode::SUCCESS;
    };

    // Rust 的 Result 终止实现会输出 Debug source 链；这里只允许固定的脱敏 Display。
    eprintln!("{error}");
    if error.requires_immediate_exit() {
        std::process::exit(1);
    }
    ExitCode::FAILURE
}
