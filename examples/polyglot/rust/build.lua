-- The rust plugin runs cargo in this folder and copies the binary to out/rust/cli.
-- src/main.rs includes out/schema/gen.rs, which //schema:gen writes, so the test needs it too.
-- The release profile: the dev one gave a binary whose bytes differed on a second run, which
-- `buildl verify` reports.
local rust = buildl.use("rust")

rust.binary("cli", { deps = { "//schema:gen" }, profile = "release" })
rust.test("cli_test", { deps = { "//schema:gen" } })
