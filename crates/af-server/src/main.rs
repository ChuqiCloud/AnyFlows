//! AnyFlows 服务进程入口。

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    af_server::install_process_safety_hooks();
    if let Some(code) = af_server::software_update_command().await {
        return code;
    }
    let _launcher_guard = match af_launcher::activate_child().await {
        Ok(guard) => guard,
        Err(_) => {
            eprintln!("启动器控制通道不可用，已停止主程序启动。");
            return ExitCode::FAILURE;
        }
    };
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
