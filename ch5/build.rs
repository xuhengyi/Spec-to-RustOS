use std::{env, fs, path::PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let linker_script_path = out_dir.join("linker.ld");
    fs::write(&linker_script_path, linker::SCRIPT).unwrap();
    println!("cargo:rustc-link-arg=-T{}", linker_script_path.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=LOG");
    println!("cargo:rerun-if-env-changed=APP_ASM");
    println!("cargo:rerun-if-env-changed=INIT_APP");
}
