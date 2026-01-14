//! Build script for compiling protobuf definitions

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = "src/proto";

    // Ensure output directory exists
    std::fs::create_dir_all(out_dir)?;

    // Compile proto files
    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .build_transport(false) // Disable transport to avoid connect method conflict
        .out_dir(out_dir)
        .compile_protos(
            &[
                "../proto/workspace/v1/agent.proto",
            ],
            &["../proto"],
        )?;

    println!("cargo:rerun-if-changed=../proto");
    Ok(())
}
