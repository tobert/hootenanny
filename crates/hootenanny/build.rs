use std::env;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=src/event.capnp");

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("event_capnp.rs");

    println!("cargo:warning=OUT_DIR is {}", out_dir);
    println!("cargo:warning=Generated file will be at {}", dest_path.display());

    capnpc::CompilerCommand::new()
        .file("src/event.capnp")
        .src_prefix("src") // Add this line
        .output_path(&out_dir)
        .run()
        .expect("compiling schema");

    if !dest_path.exists() {
        panic!("Generated file was not created at {}!", dest_path.display());
    }
    println!("cargo:warning=Successfully generated {}!", dest_path.display());
}
