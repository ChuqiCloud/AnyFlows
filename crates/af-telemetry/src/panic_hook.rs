use std::{io::Write as _, panic};

const REDACTED_PANIC_MESSAGE: &[u8] = "AnyFlows 检测到进程 panic，原始信息已隐藏\n".as_bytes();

/// 安装不输出 panic payload、源码位置或线程名的进程级 hook。
///
/// 本函数会替换既有 hook，且允许重复调用。服务启动边界必须在派生任何后台任务前
/// 调用，后续代码不得再安装会输出原始 panic 信息的 hook。
pub fn install_redacted_panic_hook() {
    panic::set_hook(Box::new(|_| {
        // panic 路径禁止格式化 payload；写入失败也不能引发二次 panic。
        let _ = std::io::stderr().lock().write_all(REDACTED_PANIC_MESSAGE);
    }));
}

#[cfg(test)]
mod tests {
    use std::{env, process::Command};

    use super::*;

    const CHILD_MARKER: &str = "ANYFLOWS_REDACTED_PANIC_CHILD";
    const SECRET_CANARY: &str = "panic-payload-secret-canary";

    #[test]
    fn panic_hook_never_prints_payload_or_location() {
        if env::var_os(CHILD_MARKER).is_some() {
            install_redacted_panic_hook();
            panic!("{SECRET_CANARY}");
        }

        let output = Command::new(env::current_exe().unwrap())
            .arg("--exact")
            .arg("panic_hook::tests::panic_hook_never_prints_payload_or_location")
            .arg("--nocapture")
            .env(CHILD_MARKER, "1")
            .output()
            .expect("必须能启动隔离 panic hook 测试进程");
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("原始信息已隐藏"));
        assert!(!stderr.contains(SECRET_CANARY));
        assert!(!stderr.contains("panic_hook.rs"));
    }
}
