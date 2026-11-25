// cargo-gb links every `*.ld` in a build script's OUT_DIR after gb-rt's `gb.ld`.
fn main() {
    let out = std::env::var("OUT_DIR").unwrap();
    std::fs::copy("shade.ld", std::path::Path::new(&out).join("shade.ld")).unwrap();
    println!("cargo:rerun-if-changed=shade.ld");
}
