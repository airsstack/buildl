//! Links the C library `//c:clamp` built.
//!
//! The target that runs cargo depends on `//c:clamp`, so buildl puts its output,
//! `out/c/libclamp.a`, in the sandbox, which mirrors the workspace: two folders up from this
//! crate, then `out/c`.

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    println!("cargo:rustc-link-search=native={manifest}/../../out/c");
    println!("cargo:rustc-link-lib=static=clamp");
}
