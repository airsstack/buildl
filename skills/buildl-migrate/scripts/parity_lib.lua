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

-- Raises `parity.json: <field> <what is wrong>`; the message is the whole error, with no
-- position prefix.
local function refuse(field, problem)
    error("parity.json: " .. field .. " " .. problem, 0)
end

-- A list is a table whose keys are exactly 1..n. A decoded `[]` and `{}` are both empty tables.
local function is_list(value)
    if type(value) ~= "table" then
        return false
    end
    local count = 0
    for _ in pairs(value) do
        count = count + 1
    end
    return count == #value
end

local function require_string_list(value, field, non_empty)
    if not is_list(value) then
        refuse(field, "must be a list of strings")
    end
    if non_empty and #value == 0 then
        refuse(field, "must not be empty")
    end
    for i, item in ipairs(value) do
        if type(item) ~= "string" then
            refuse(field .. "[" .. i .. "]", "must be a string")
        end
    end
end

local function require_non_empty_string(value, field)
    if type(value) ~= "string" or value == "" then
        refuse(field, "must be a non-empty string")
    end
end

local function require_table(value, field)
    if type(value) ~= "table" then
        refuse(field, "is required and must be a table")
    end
end

local function check_old(old)
    require_table(old, "old")
    require_string_list(old.build, "old.build", true)
    if old.test ~= nil then
        require_string_list(old.test, "old.test", true)
    end
    require_string_list(old.programs, "old.programs")
    require_string_list(old.env, "old.env")
    if type(old.network) ~= "boolean" then
        refuse("old.network", "is required and must be true or false")
    end
end

-- Each recipe maps to a buildl target, or is dropped with a reason. Checked in name order so
-- the first bad recipe named is the same on every run.
local function check_recipes(recipes)
    require_table(recipes, "recipes")
    local names = {}
    for name in pairs(recipes) do
        names[#names + 1] = name
    end
    table.sort(names)
    for _, name in ipairs(names) do
        local field = "recipes." .. name
        local entry = recipes[name]
        require_table(entry, field)
        if entry.target ~= nil and entry.dropped ~= nil then
            refuse(field, "must hold either target or dropped, not both")
        elseif entry.target ~= nil then
            require_non_empty_string(entry.target, field .. ".target")
        elseif entry.dropped ~= nil then
            require_non_empty_string(entry.dropped, field .. ".dropped")
        else
            refuse(field, "must hold target or dropped")
        end
    end
end

local function check_outputs(outputs)
    if not is_list(outputs) then
        refuse("outputs", "is required and must be a list")
    end
    for i, entry in ipairs(outputs) do
        local field = "outputs[" .. i .. "]"
        require_table(entry, field)
        require_non_empty_string(entry.old, field .. ".old")
        require_non_empty_string(entry.new, field .. ".new")
        if entry.judge ~= nil then
            if entry.judge ~= "tests" then
                refuse(field .. ".judge", 'must be "tests"')
            end
            require_non_empty_string(entry.reason, field .. ".reason")
        end
    end
end

-- Returns `decoded`, a decoded `parity.json`, once every field is known to be fit; raises
-- naming the first field that is not. `old.test` and `skip` may be absent.
function M.config(decoded)
    require_table(decoded, "the file")
    if LISTERS[decoded.tool] == nil then
        refuse("tool", "must be one of make, just, shell, npm")
    end
    check_old(decoded.old)
    check_recipes(decoded.recipes)
    check_outputs(decoded.outputs)
    if decoded.skip ~= nil then
        require_string_list(decoded.skip, "skip")
    end
    return decoded
end

return M
