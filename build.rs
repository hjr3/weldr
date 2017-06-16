extern crate capnpc;

fn main() {
    ::capnpc::CompilerCommand::new()
        .file("weldr.capnp")
        .run()
        .unwrap();
}
