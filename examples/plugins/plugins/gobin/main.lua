local module = {}

-- Declares a target building the Go program in directory `dir` of the module whose go.mod sits
-- beside the calling build file. The Go toolchain keeps its build cache under $HOME, so the target
-- reads HOME, as `unkeyed`: the value goes to `go`, only the name enters the key.
function module.binary(name, dir)
  buildl.target(name, {
    inputs = { "go.mod", buildl.sources(dir .. "/*.go") },
    run    = { "go", "build", "-o", "$out", "./" .. dir },
    env    = { HOME = "unkeyed" },
  })
end

return module
