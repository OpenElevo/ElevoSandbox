//! Build script for compiling protobuf definitions

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = "src/proto";

    // Ensure output directory exists
    std::fs::create_dir_all(out_dir)?;

    // Compile proto files
    tonic_build::configure()
        .build_server(true)
        .build_client(false)  // Don't build client for server
        .out_dir(out_dir)
        .compile_protos(
            &[
                "../proto/workspace/v1/sandbox.proto",
                "../proto/workspace/v1/process.proto",
                "../proto/workspace/v1/pty.proto",
                "../proto/workspace/v1/agent.proto",
            ],
            &["../proto"],
        )?;

    println!("cargo:rerun-if-changed=../proto");
    Ok(())
}
