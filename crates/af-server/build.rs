use std::{
    env, fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

const TEST_INDEX: &str = "<!doctype html><html lang=\"zh-CN\"><meta charset=\"UTF-8\"><title>AnyFlows</title><body data-anyflows-build-fixture></body></html>";

fn main() {
    let manifest_directory = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo 必须提供 CARGO_MANIFEST_DIR"),
    );
    let workspace_directory = manifest_directory
        .parent()
        .and_then(Path::parent)
        .expect("af-server 必须位于工作区 crates 目录下");
    println!("cargo:rerun-if-env-changed=PROFILE");
    for frontend in ["web", "web-next"] {
        let source = workspace_directory.join(frontend).join("dist");
        let destination = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo 必须提供 OUT_DIR"))
            .join(format!("{frontend}-dist"));
        println!("cargo:rerun-if-changed={}", source.display());
        prepare_destination(&destination).expect("无法准备前端内嵌目录");
        if source.join("index.html").is_file() {
            copy_directory(&source, &destination).expect("无法复制前端构建产物");
        } else if env::var("PROFILE").as_deref() == Ok("release") {
            panic!("release 构建缺少 {frontend}/dist/index.html，请先构建两套内置前端");
        } else {
            fs::write(destination.join("index.html"), TEST_INDEX).expect("无法写入前端测试夹具");
        }
    }
}

fn prepare_destination(destination: &Path) -> io::Result<()> {
    match fs::remove_dir_all(destination) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::create_dir_all(destination)
}

fn copy_directory(source: &Path, destination: &Path) -> io::Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            fs::create_dir_all(&destination_path)?;
            copy_directory(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(source_path, destination_path)?;
        } else {
            return Err(io::Error::other(format!(
                "前端产物包含不支持的文件类型: {}",
                source_path.display()
            )));
        }
    }
    Ok(())
}
