fn main() {
    println!("cargo:rerun-if-changed=schemas/");

    capnpc::CompilerCommand::new()
        .src_prefix("schemas")
        .file("schemas/common.capnp")
        .file("schemas/jobs.capnp")
        .file("schemas/tools.capnp")
        .file("schemas/streams.capnp")
        .file("schemas/envelope.capnp")
        .file("schemas/garden.capnp")
        .file("schemas/broadcast.capnp")
        .run()
        .expect("capnp compile failed");
}
