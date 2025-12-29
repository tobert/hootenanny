fn main() {
    // Watch each schema file individually - directory watching only catches add/remove,
    // not content changes
    let schemas = [
        "schemas/common.capnp",
        "schemas/jobs.capnp",
        "schemas/tools.capnp",
        "schemas/streams.capnp",
        "schemas/responses.capnp",
        "schemas/envelope.capnp",
        "schemas/garden.capnp",
        "schemas/broadcast.capnp",
        "schemas/vibeweaver.capnp",
    ];

    for schema in &schemas {
        println!("cargo:rerun-if-changed={}", schema);
    }

    let mut cmd = capnpc::CompilerCommand::new();
    cmd.src_prefix("schemas");
    for schema in &schemas {
        cmd.file(schema);
    }
    cmd.run().expect("capnp compile failed");
}
