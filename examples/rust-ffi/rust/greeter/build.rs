//! Gives the shared library a runtime name a binary can find it by.
//!
//! Left alone, the library is named by the path the linker wrote it to, inside the folder cargo
//! ran in: a binary linked against it records that path and fails to start once the folder is
//! gone. Named `libgreeter.so` (Linux, the soname) or `@rpath/libgreeter.dylib` (macOS, the
//! install name), it is found through the binary's rpath instead.

fn main() {
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("linux") => println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,libgreeter.so"),
        Ok("macos") => {
            println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libgreeter.dylib");
        }
        _ => {}
    }
}
