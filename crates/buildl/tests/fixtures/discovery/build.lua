local b = buildl
b.discover("pkgs", { inputs = { "packages.json" }, run = { "cp", "$in", "$out" } })

local text = b.discovered("//:pkgs")
local names = text and airsstack.json.decode(text) or {}
for _, name in ipairs(names) do
  b.target(name, { inputs = { "item.txt" }, run = { "cp", "$in", "$out" } })
end
