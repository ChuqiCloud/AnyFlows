fn main() {
    println!(
        "cargo:rustc-env=AF_LAUNCHER_TARGET={}",
        std::env::var("TARGET").expect("Cargo TARGET")
    );
}
