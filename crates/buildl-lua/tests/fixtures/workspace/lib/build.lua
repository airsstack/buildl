buildl.target("text", {
  run    = { "cc", "-c", "$in", "-o", "$out" },
  inputs = buildl.sources("src/*.c"),
})
