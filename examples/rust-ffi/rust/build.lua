local rust = buildl.use("rust")

-- Two crates of one Cargo workspace, each a library for C. `rust.package` picks the member;
-- the name of each call is the crate's, so the plugin finds the file cargo wrote. release,
-- not the default dev, so `buildl verify` finds the same bytes: dev debug info keeps the
-- absolute folder cargo ran in.
rust.library("mathlib", {
  kind = "staticlib",
  args = rust.package("mathlib"),
  profile = "release",
})

rust.test("unit")

-- The shared library's file name depends on the platform, which the first pass does not know
-- yet: rust.platform() declares the discovery that asks rustc, and returns nil until it has run.
local platform = rust.platform()
if platform == nil then
  return
end

rust.library("greeter", {
  kind = "cdylib",
  args = rust.package("greeter"),
  profile = "release",
  output = rust.shared_name("greeter", platform),
})
