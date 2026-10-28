local b = buildl

local sources = { "Cargo.toml", "Cargo.lock", b.sources("src/*.rs") }

-- cargo reads HOME for its registry and toolchain, and PATH to find the linker.
-- `cargo install --root $out` puts the binary at out/hello/bin/hello; cargo's own
-- build directory, tt, stays inside the action's sandbox. See README.md.
b.target("hello", {
  inputs = sources,
  run = { "cargo", "install", "--path", ".", "--root", "$out", "--target-dir", "tt", "--no-track" },
  env = { "HOME", "PATH" },
})

b.test("unit", {
  inputs = sources,
  run = { "cargo", "test", "--target-dir", "tt" },
  env = { "HOME", "PATH" },
})
