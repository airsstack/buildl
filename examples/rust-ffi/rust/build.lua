local rust = buildl.use("rust")

-- Three crates of one Cargo workspace. `rust.package` picks the member; the name of each call
-- is the crate's, so the plugin finds the file cargo wrote. release, not the default dev, so
-- `buildl verify` finds the same bytes: dev debug info keeps the absolute folder cargo ran in.

-- A static library C calls.
rust.library("mathlib", {
  kind = { "staticlib" },
  args = rust.package("mathlib"),
  profile = "release",
})

-- What follows waits for the second pass. rust.platform() declares the discovery that asks
-- rustc and returns nil until it has run; the plugin names the shared library's file after it,
-- as cargo does. And //c:clamp is declared by the c plugin on its second pass only, once its compiler
-- discovery has run, so a target depending on it is declared then too.
local platform = rust.platform()
if platform == nil then
  return
end

-- A shared library C calls.
rust.library("greeter", {
  kind = { "cdylib" },
  args = rust.package("greeter"),
  profile = "release",
})

-- The other direction: a Rust binary calling a C library. Its target depends on //c:clamp, so
-- out/c/libclamp.a is in the sandbox when cargo runs, and calc/build.rs links it.
rust.binary("calc", {
  args = rust.package("calc"),
  deps = { "//c:clamp" },
  profile = "release",
})

-- cargo test builds every crate of the workspace, calc too, so it needs the C library as well.
rust.test("unit", { deps = { "//c:clamp" } })
