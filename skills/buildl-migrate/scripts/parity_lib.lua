-- The pure half of the parity check: it reads no file and takes plain tables and strings.
-- `parity.lua` does the file work and calls this.
local M = {}

-- Calls `fn(line)` for each line of `text`, without its line ending.
local function each_line(text, fn)
    for raw in (text .. "\n"):gmatch("(.-)\r?\n") do
        fn(raw)
    end
end

-- Returns the keys of the set `seen` as a sorted list.
local function sorted(seen)
    local list = {}
    for name in pairs(seen) do
        list[#list + 1] = name
    end
    table.sort(list)
    return list
end

-- The header of an assignment or rule line splits at its first colon. `:=`, `::=` and `:::=`
-- are assignments, not rules.
local function is_colon_assignment(line, colon)
    local rest = line:sub(colon + 1)
    return rest:match("^:*=") ~= nil
end

-- make: every target at the start of a line, pattern rules included, without `.PHONY` and the
-- other special targets that start with `.`, and without variable assignments.
local function make_recipes(files)
    local file = files["GNUmakefile"] or files["makefile"] or files["Makefile"] or ""
    local seen = {}
    each_line(file, function(line)
        if line:match("^[%s#]") or line == "" then
            return
        end
        local colon = line:find(":", 1, true)
        if colon == nil or is_colon_assignment(line, colon) then
            return
        end
        local header = line:sub(1, colon - 1)
        if header:find("=", 1, true) then
            return
        end
        for name in header:gmatch("%S+") do
            if name:sub(1, 1) ~= "." then
                seen[name] = true
            end
        end
    end)
    return sorted(seen)
end

local JUST_WORDS = { set = true, alias = true, export = true, import = true, mod = true }

-- just: every recipe header (`name args:`), without `set`, `alias`, `export` and `:=`
-- assignments.
local function just_recipes(files)
    local seen = {}
    each_line(files["justfile"] or "", function(line)
        local name = line:match("^@?([%a_][%w_-]*)")
        if name == nil or JUST_WORDS[name] then
            return
        end
        local colon = line:find(":", 1, true)
        if colon == nil or is_colon_assignment(line, colon) then
            return
        end
        seen[name] = true
    end)
    return sorted(seen)
end

-- npm: the keys of `package.json`'s `scripts`.
local function npm_recipes(files)
    local decoded = airsstack.json.decode(files["package.json"] or "{}")
    local seen = {}
    for key in pairs(decoded.scripts or {}) do
        seen[key] = true
    end
    return sorted(seen)
end

-- shell: every `*.sh` path.
local function shell_recipes(files)
    local seen = {}
    for rel in pairs(files) do
        if rel:match("%.sh$") then
            seen[rel] = true
        end
    end
    return sorted(seen)
end

local LISTERS = {
    make = make_recipes,
    just = just_recipes,
    npm = npm_recipes,
    shell = shell_recipes,
}

-- Returns the sorted recipe names of an old build. `files` maps a path relative to the project
-- to its text; `tool` is `make`, `just`, `shell` or `npm`. Raises naming any other tool.
function M.recipes(tool, files)
    local lister = LISTERS[tool]
    if lister == nil then
        error("unknown build tool: " .. tostring(tool), 0)
    end
    return lister(files)
end

return M
