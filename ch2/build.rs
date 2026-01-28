use std::{env, fs, path::PathBuf};

fn main() {
    // 输出目录由 Cargo 提供
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // 将链接脚本写入 OUT_DIR
    let linker_script_path = out_dir.join("linker.ld");
    fs::write(&linker_script_path, linker::SCRIPT).expect("Failed to write linker.ld");

    // 将链接参数传递给链接器
    println!(
        "cargo:rustc-link-arg=-T{}",
        linker_script_path.display()
    );

    // 当 build.rs 变化时重新构建
    println!("cargo:rerun-if-changed=build.rs");

    // 当环境变量变化时重新构建
    println!("cargo:rerun-if-env-changed=LOG");
    println!("cargo:rerun-if-env-changed=APP_ASM");
}
