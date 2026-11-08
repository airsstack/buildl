local b = buildl

local app = b.sources("app/*.py")

-- zipapp packs the app directory into one runnable archive, out/app.pyz. python3 reads HOME
-- and PATH, which differ from one machine to the next, so both are unkeyed: they reach the
-- action when it runs here but stay out of its key, and a checkout on another machine fetches
-- the same result. Under remote execution unkeyed variables are not sent, so the worker runs
-- with its own environment.
b.target("app.pyz", {
  inputs = { app },
  run = { "python3", "-m", "zipapp", "app", "-o", "$out" },
  env = { HOME = "unkeyed", PATH = "unkeyed" },
})

b.test("unit", {
  inputs = { app, b.sources("tests/*.py") },
  run = { "python3", "-m", "unittest", "discover", "-s", "tests" },
  env = { HOME = "unkeyed", PATH = "unkeyed" },
})
