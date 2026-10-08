local b = buildl

b.subdir("lib")

b.rule("cc", {
  run  = { "cc", "-c", "$in", "-o", "$out" },
  desc = "compile $in",
})

b.target("main.o", { rule = "cc", inputs = { "src/main.c" } })

b.target("app", {
  deps    = { "main.o", "//lib:text" },
  run     = { "cc", "$deps", "-o", "$out" },
  outputs = { "app" },
  env     = { "HOME" },
  network = true,
  always  = true,
})

b.test("app_test", { deps = { "app" }, run = { "$out/app", "--self-test" } })

b.alias("default", "app")

b.option("mode", { default = "debug" })
