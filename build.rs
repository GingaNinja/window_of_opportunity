#[cfg(target_os = "windows")]
extern crate embed_resource;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(feature, values(\"cargo-clippy\"))");
    if cfg!(target_os = "windows") {
        embed_resource::compile_for_everything("app.rc", embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
}
