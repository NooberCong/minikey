fn main() {
    println!("cargo:rerun-if-changed=assets");
    embed_resource::compile("assets/minikey.rc", embed_resource::NONE)
        .manifest_required()
        .unwrap();
}
