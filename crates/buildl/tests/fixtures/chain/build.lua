local b = buildl
b.subdir("lib")
b.option("order", { default = "-u" })
b.alias("text", "//lib:words")

b.target("greet", {                       -- executable output
  inputs = { "greet.sh" },
  env    = { "BUILDL_FIXTURE_FLAVOR" },
  run    = { "cp", "$in", "$out" },
})
b.target("greeting", {                    -- runs what greet built, through the alias
  deps = { "greet", "text" },
  run  = { "$deps", "$out" },
})
b.target("report", {
  deps   = { "greeting" },
  inputs = { "footer.txt" },
  run    = { "sort", "-o", "$out", "$deps", "$in" },
})
b.target("site", {                        -- a directory from a directory
  deps = { "//lib:assets" },
  run  = { "cp", "-R", "$deps", "$out" },
})
