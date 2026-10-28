local b = buildl

-- A command reads its paths from the workspace root, so go builds the module in go/ with -C
-- and the output path is written from there. go.mod's replace line points at
-- out/schema/gogen, where the Go output of //schema:gen is placed.
b.target("service", {
  inputs = { "go.mod", b.sources("*.go") },
  deps = { "//schema:gen" },
  run = { "go", "build", "-C", "go", "-o", "../$out", "." },
  env = { "HOME" },
})
