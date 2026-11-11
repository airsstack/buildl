-- buildl builds buildl, with the release `.buildl-version` names.
-- Written by hand, not by `cargo buildl init` or `cargo buildl sync`.
local rust = buildl.use("rust")

rust.binary("buildl", { args = rust.package("buildl-cli"), version_probe = true })
rust.binary("cargo-buildl", { args = rust.package("buildl-cargo"), version_probe = true })
rust.test("tests", {
  args = { "--workspace", "--all-targets", "--all-features" },
  version_probe = true,
})
rust.test("doctests", {
  args = { "--workspace", "--all-features", "--doc" },
  version_probe = true,
})
