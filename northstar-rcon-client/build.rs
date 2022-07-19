use protobuf_codegen::Codegen;

fn main() {
    Codegen::new()
        .pure()
        .cargo_out_dir("protos")
        .input("protos/cl_rcon.proto")
        .input("protos/sv_rcon.proto")
        .include("protos")
        .run_from_script();
}
