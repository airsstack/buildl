local rust = buildl.use("rust")

-- One build file at the root of the Cargo workspace, one call per artifact. The rlib
-- (greeting) and the proc-macro (answer) get no call of their own: nothing but rustc reads
-- them, so cargo builds them on the way to the binary and nothing is copied to out/.

-- The binary. Its args are a list the build file composes: rust.package picks the member, and
-- rust.features turns on greeting's `shout` (cargo applies a feature named crate/feature).
-- release, not the default dev, so `buildl verify` finds the same bytes: dev debug info keeps
-- the absolute folder cargo ran in.
local args = rust.package("cli")
for _, arg in ipairs(rust.features({ "greeting/shout" })) do
  args[#args + 1] = arg
end
rust.binary("cli", { args = args, profile = "release" })

-- Every crate's tests, the rlib and the proc-macro included: cargo test --workspace.
rust.test("unit", { args = { "--workspace" } })

-- One crate's tests on their own, with the feature on: cargo test -p greeting --features shout.
local greeting_args = rust.package("greeting")
for _, arg in ipairs(rust.features({ "shout" })) do
  greeting_args[#greeting_args + 1] = arg
end
rust.test("greeting_shout", { args = greeting_args })
