// build.rs — 自动 fallback 预编译 proto 代码
//
// 策略：
// 1. 如果 protoc 可用 → 正常编译 proto
// 2. 如果 protoc 不可用 → 复制 src/generated.rs 到 OUT_DIR/proto.rs
//
// 这样 CI 不需要安装 protoc，本地开发有 protoc 会自动生成最新代码

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_file = std::path::Path::new(&out_dir).join("proto.rs");

    // 尝试使用 protoc 编译
    let result = tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["../../proto/nezha.proto"], &["../../proto/"]);

    match result {
        Ok(_) => {
            // protoc 成功，更新预编译代码
            if out_file.exists() {
                let _ = std::fs::copy(&out_file, "src/generated.rs");
            }
        }
        Err(e) => {
            // protoc 不可用，使用预编译代码
            println!("cargo:warning=protoc not available ({e}), using pre-generated code");
            let pre = std::path::Path::new("src/generated.rs");
            if pre.exists() {
                std::fs::copy(pre, &out_file)?;
            } else {
                return Err(format!(
                    "protoc not found and src/generated.rs missing. Install protoc: apt install protobuf-compiler"
                ).into());
            }
        }
    }

    println!("cargo:rerun-if-changed=../../proto/nezha.proto");
    Ok(())
}
