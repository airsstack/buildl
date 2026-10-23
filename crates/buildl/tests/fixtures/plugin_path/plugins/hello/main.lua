local module = {}

-- Declares a target that copies `hello.txt` into the output directory.
function module.greet(name)
  buildl.target(name, {
    inputs = { "hello.txt" },
    run    = { "cp", "$in", "$out" },
  })
end

return module
