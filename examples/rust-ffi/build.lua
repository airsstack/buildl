-- Each folder declares its own targets: rust/ the two Rust libraries, c/ the C programs that
-- use them. They reach each other by label. Keeping them apart matters: the rust plugin takes
-- every file of its folder as an input, so a C header beside the crates would rerun cargo.
buildl.subdir("rust")
buildl.subdir("c")
