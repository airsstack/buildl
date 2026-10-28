local b = buildl

local sources = { "go.mod", b.sources("*.go") }

-- The Go toolchain keeps its build cache under $HOME, so both actions read HOME.
b.target("hello", {
  inputs = sources,
  run = { "go", "build", "-o", "$out", "." },
  env = { "HOME" },
})

b.test("unit", {
  inputs = sources,
  run = { "go", "test", "./..." },
  env = { "HOME" },
})
