fn main() {
    capnpc::CompilerCommand::new()
        .src_prefix("schema")
        .file("schema/samskara_editor.capnp")
        .file("schema/samskara_world.capnp")
        .run()
        .expect("compiling schema");
}
