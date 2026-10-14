local b = buildl
b.rule("merge", {
  run  = { "sort", "$opt:order", "-o", "$out", "$in" },
  desc = "merge $in",
})
b.target("words",  { rule = "merge", inputs = b.sources("words/*.txt") })   -- file output
b.target("assets", {                                                        -- directory output
  inputs = b.sources("assets/**/*.txt"),
  run    = { "cp", "-R", "lib/assets", "$out" },
})
