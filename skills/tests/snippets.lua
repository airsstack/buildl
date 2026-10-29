-- Writes one buildl workspace per checkable lua block in the skills, for `buildl check`.
-- Run from the repository root:
--   airsl run --allow-read skills --allow-write W skills/tests/snippets.lua W
-- Each block lands in W/<md path, / as ->-<index>/build.lua beside a buildl.toml. The
-- fixtures under skills/tests/fixtures/ are test inputs, not skill content, and are left out.
local fs = airsstack.fs
local path = airsstack.path
local snippets = require("snippets_lib")

local ROOT = "skills"
local FIXTURES = "tests/fixtures/"

-- `check` does not apply the ceiling, so the program list stays empty.
local TOML = [[
[workspace]
name   = "snippet"
entry  = "build.lua"
out    = "out"
config = 1

[ceiling]
proc.run = []
env.read = []
fs.read  = ["."]
fs.write = ["out", ".buildl"]
]]

local out = arg[1]
if out == nil then
    error("usage: snippets.lua W", 0)
end

for _, rel in ipairs(fs.walk(ROOT)) do
    if path.ext(rel) == "md" and rel:sub(1, #FIXTURES) ~= FIXTURES then
        local file = path.join(ROOT, rel)
        for _, block in ipairs(snippets.blocks(fs.read(file), file)) do
            local dir = path.join(out, rel:gsub("/", "-") .. "-" .. block.index)
            fs.mkdir(dir)
            fs.write(path.join(dir, "build.lua"), block.body)
            fs.write(path.join(dir, "buildl.toml"), TOML)
            print(string.format("%s: %s block %d, line %d", dir, file, block.index, block.line))
        end
    end
end
