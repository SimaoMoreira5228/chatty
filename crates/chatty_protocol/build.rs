use std::env;
use std::path::PathBuf;

fn main() {
	println!("cargo:rerun-if-changed=../../proto");
	println!("cargo:rerun-if-changed=../../proto/chatty.proto");

	let proto_dir = PathBuf::from("../../proto");
	let proto_file = proto_dir.join("chatty.proto");

	let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be set by Cargo"));

	let protos = [proto_file];
	let includes = [proto_dir];

	let mut config = prost_build::Config::new();

	config.out_dir(out_dir);
	config.protoc_arg("--experimental_allow_proto3_optional");
	config.type_attribute(
		".chatty.v1.EventEnvelope.Event",
		"#[allow(clippy::large_enum_variant)]",
	);

	config
		.compile_protos(&protos, &includes)
		.expect("failed to compile protobuf definitions with prost");
}
