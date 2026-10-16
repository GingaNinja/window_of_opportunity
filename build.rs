#[cfg(target_os = "windows")]
extern crate embed_resource;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(feature, values(\"cargo-clippy\"))");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for_examples("app.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
