-- Checks the skill paths .claude-plugin/marketplace.json lists, which
-- `claude plugin validate` leaves unchecked. Run from the repository root:
--   airsl run --allow-read . skills/tests/marketplace.lua
-- Every listed folder must hold a SKILL.md, and every skills/*/ folder holding one must be listed.
local fs = airsstack.fs
local json = airsstack.json
local path = airsstack.path

local MANIFEST = ".claude-plugin/marketplace.json"

local listed = {}
local manifest = json.decode(fs.read(MANIFEST))
if type(manifest.plugins) ~= "table" then
    error(MANIFEST .. ": has no plugins list", 0)
end

for _, plugin in ipairs(manifest.plugins) do
    -- The manifest allows a single path as a string or a list of paths.
    local dirs = plugin.skills or {}
    if type(dirs) == "string" then
        dirs = { dirs }
    end
    for _, dir in ipairs(dirs) do
        local rel = dir:gsub("^%./", ""):gsub("/$", "")
        if not fs.is_file(path.join(rel, "SKILL.md")) then
            error(string.format("%s: %s holds no SKILL.md", MANIFEST, dir), 0)
        end
        listed[rel] = true
    end
end

for _, name in ipairs(fs.list("skills")) do
    local rel = path.join("skills", name)
    if fs.is_file(path.join(rel, "SKILL.md")) and not listed[rel] then
        error(string.format("%s: %s holds a SKILL.md but is not listed", MANIFEST, rel), 0)
    end
end
