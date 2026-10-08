buildl.subdir("lib")

buildl.target("app", {
  deps   = { "//lib:text" },
  run    = { "cc", "$in", "$deps", "-o", "$out" },
  inputs = buildl.sources("*.c"),
})
