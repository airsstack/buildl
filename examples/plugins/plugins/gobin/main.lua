local module = {}

-- Declares a target building the Go program in directory `dir` of the module
-- whose go.mod sits beside the calling build file. The Go toolchain keeps its
-- build cache under $HOME, so the target reads HOME.
function module.binary(name, dir)
  buildl.target(name, {
    inputs = { "go.mod", buildl.sources(dir .. "/*.go") },
    run    = { "go", "build", "-o", "$out", "./" .. dir },
    env    = { "HOME" },
  })
end

return module
