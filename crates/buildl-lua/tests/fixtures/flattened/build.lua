buildl.target("app", {
  run    = { "cc", "$in", "-o", "$out" },
  inputs = { buildl.sources("src/*.c"), "go.mod" },
})
