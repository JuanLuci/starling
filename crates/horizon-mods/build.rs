//! Embeds Starling's shared JavaScript engine when it has been built
//! (`node crates/starling-mod-sdk/js-engine/build.mjs`, or `STARLING_JS_ENGINE`): compressed, with
//! the digest of the component, so the game reads it back from its compiled-mod cache without
//! unpacking it. Without it the build runs component mods only and refuses JavaScript ones.
use sha2::Digest;
use std::{env, fs, io::Write, path::PathBuf};

fn main() {
    println!("cargo:rustc-check-cfg=cfg(js_engine)");
    println!("cargo:rerun-if-env-changed=STARLING_JS_ENGINE");
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let engine = env::var_os("STARLING_JS_ENGINE").map_or_else(
        || manifest.join("../../target/js-engine/engine.wasm"),
        PathBuf::from,
    );
    println!("cargo:rerun-if-changed={}", engine.display());
    if env::var_os("CARGO_FEATURE_RUNTIME").is_none() {
        return;
    }
    let Ok(wasm) = fs::read(&engine) else {
        return;
    };
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(&wasm).unwrap();
    fs::write(out.join("js-engine.wasm.gz"), gz.finish().unwrap()).unwrap();
    let digest: String = sha2::Sha256::digest(&wasm)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    fs::write(
        out.join("js_engine.rs"),
        format!("/// SHA-256 of the engine component.\npub const DIGEST: &str = \"{digest}\";\n"),
    )
    .unwrap();
    println!("cargo:rustc-cfg=js_engine");
}
