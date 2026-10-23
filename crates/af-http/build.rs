use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=AF_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=AF_BUILD_TIME");
    println!("cargo:rerun-if-changed=build.rs");
    for git_path in ["HEAD", "packed-refs"] {
        watch_git_path(git_path);
    }
    if let Some(reference) = git_value("symbolic-ref", &["-q", "HEAD"]) {
        watch_git_path(&reference);
    }

    emit_metadata("AF_BUILD_COMMIT", "rev-parse", &["HEAD"], valid_commit);
    let build_time = std::env::var("AF_BUILD_TIME")
        .ok()
        .filter(|value| valid_build_time(value));
    println!(
        "cargo:rustc-env=AF_BUILD_TIME={}",
        build_time.unwrap_or_default()
    );
}

fn watch_git_path(path: &str) {
    if let Some(path) = git_value("rev-parse", &["--path-format=absolute", "--git-path", path]) {
        println!("cargo:rerun-if-changed={path}");
    }
}

fn emit_metadata(name: &str, git_command: &str, arguments: &[&str], validate: fn(&str) -> bool) {
    let supplied = std::env::var(name).ok();
    let value = supplied
        .map(|value| value.trim().to_owned())
        .filter(|value| validate(value))
        .or_else(|| git_value(git_command, arguments).filter(|value| validate(value)));
    println!(
        "cargo:rustc-env={name}={}",
        value.as_deref().unwrap_or_default()
    );
}

fn git_value(command: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(std::env::var_os("CARGO_MANIFEST_DIR")?)
        .arg(command)
        .args(arguments)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_owned())
}

fn valid_commit(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_build_time(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-+:.TZ".contains(&byte))
}
