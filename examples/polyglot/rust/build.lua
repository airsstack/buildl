local b = buildl

-- cargo install puts the binary at out/rust/cli/bin/cli. src/main.rs includes
-- out/schema/gen.rs, which //schema:gen writes.
b.target("cli", {
  inputs = { "Cargo.toml", "Cargo.lock", b.sources("src/*.rs") },
  deps = { "//schema:gen" },
  run = { "cargo", "install", "--path", "rust", "--root", "$out", "--target-dir", "tt", "--no-track" },
  env = { "HOME", "PATH" },
})
