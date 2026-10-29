local b = buildl

b.target("out.txt", {
  inputs = { "in.txt" },
  run = { "cp", "$in", "$out" },
})

b.test("unit", {
  deps = { ":out.txt" },
  run = { "grep", "-q", "hello", "$deps" },
})
