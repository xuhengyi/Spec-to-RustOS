use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    // 告诉 Cargo 如果这些文件改变，重新运行 build script
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/syscall.h.in");

    // 读取 syscall.h.in 文件
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let input_path = PathBuf::from(manifest_dir).join("src/syscall.h.in");
    let content = fs::read_to_string(&input_path)
        .expect("Failed to read src/syscall.h.in");

    // 解析 #define __NR_* 定义
    let mut syscalls = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("#define __NR_") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let name = parts[1].strip_prefix("__NR_").unwrap();
                let value = parts[2];
                syscalls.push((name.to_string(), value.to_string()));
            }
        }
    }

    // 生成 syscalls.rs
    let mut output = String::new();
    output.push_str("// Auto-generated file from build.rs\n");
    output.push_str("// Do not edit manually\n\n");
    output.push_str("impl crate::SyscallId {\n");

    for (name, value) in syscalls {
        // 将名称转换为大写的常量名（如 READ, WRITE）
        let const_name = name.to_uppercase();
        output.push_str(&format!("    pub const {}: crate::SyscallId = crate::SyscallId({});\n", const_name, value));
    }

    output.push_str("}\n");

    // 写入输出文件
    let output_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("src/syscalls.rs");
    
    fs::write(&output_path, output)
        .expect("Failed to write src/syscalls.rs");
}
