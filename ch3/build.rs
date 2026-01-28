use std::{env, fs, path::PathBuf};

fn main() {
    // 写入链接脚本
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let linker_script_path = out_dir.join("linker.ld");
    fs::write(&linker_script_path, linker::SCRIPT).unwrap();

    // 传递链接脚本给链接器
    println!(
        "cargo:rustc-link-arg=-T{}",
        linker_script_path.display()
    );

    // 触发重建条件
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=LOG");
    println!("cargo:rerun-if-env-changed=APP_ASM");
}
