local b = buildl
b.target("greeting", { run = { "printf", "hello\n" }, stdout = true })
b.discover("pkgs", {
  run = { "printf", "{\"name\":\"alpha\"}\n{\"name\":\"beta\"}\n" },
  stdout = true,
})

-- What //:pkgs printed is two JSON values back to back, read with decode_all: one
-- target per value, printing its name.
local listed = b.discovered(":pkgs")
if listed ~= nil then
  for _, pkg in ipairs(b.decode_all(listed)) do
    b.target(pkg.name, { run = { "printf", pkg.name .. "\n" }, stdout = true })
  end
end
