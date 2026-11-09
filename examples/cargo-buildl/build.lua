-- cargo-buildl: begin generated. `cargo buildl sync` rewrites this part from Cargo.toml.
local rust = buildl.use("rust")

rust.binary("app", { args = rust.package("app") })
rust.test("app-test", { args = rust.package("app") })
rust.library("ffi", { kind = { "staticlib" }, args = rust.package("ffi") })
rust.test("ffi-test", { args = rust.package("ffi") })
rust.test("greet-test", { args = rust.package("greet") })
-- cargo-buildl: end generated

-- Your own targets go below. `cargo buildl sync` never changes them.
