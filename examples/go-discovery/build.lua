local b = buildl

-- The module line of go.mod.
local module = "example.com/discovery"

-- go list prints one JSON object per package to stdout, so the discovery target's
-- output is what it prints. -json=ImportPath keeps only the field the build reads:
-- the full objects also hold absolute paths (Dir, Root, Module.Dir), which name the
-- action's directory and so differ from one checkout or machine to another.
b.discover("pkgs", {
  run = { "go", "list", "-json=ImportPath", "./..." },
  inputs = { "go.mod", b.sources("**/*.go") },
  env = { "HOME" },
  stdout = true,
})

-- The first pass runs before go list has: there is nothing to declare from yet.
-- The discovery target above is declared the same way on both passes.
local listed = b.discovered(":pkgs")
if listed == nil then
  return
end

-- go list's output is a stream of objects, not one JSON value, so it is read with
-- decode_all. Each package's test target is named after its directory, with '/'
-- written '.' because a target name holds no '/': text/words becomes text.words.
for _, pkg in ipairs(b.decode_all(listed)) do
  local dir = pkg.ImportPath:sub(#module + 2)
  b.test((dir:gsub("/", ".")), {
    inputs = { "go.mod", b.sources(dir .. "/*.go") },
    run = { "go", "test", "./" .. dir },
    env = { "HOME" },
  })
end
