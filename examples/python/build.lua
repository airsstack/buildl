local b = buildl

local app = b.sources("app/*.py")

-- zipapp packs the app directory into one runnable archive, out/app.pyz.
b.target("app.pyz", {
  inputs = { app },
  run = { "python3", "-m", "zipapp", "app", "-o", "$out" },
})

b.test("unit", {
  inputs = { app, b.sources("tests/*.py") },
  run = { "python3", "-m", "unittest", "discover", "-s", "tests" },
})
