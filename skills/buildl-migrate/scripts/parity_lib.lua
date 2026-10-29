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

-- Returns the ceiling of a `buildl doctor` text as `{ [key] = { values… } }`. The ceiling is
-- the run of indented lines after the line `ceiling`, one `  <key>  <values joined by ", ">`
-- per key, with `-` for none. Lines outside the run are ignored. Raises when there is no
-- `ceiling` line, or when a line in the run has another form.
function M.ceiling(doctor_text)
    local ceiling
    local reading = false
    each_line(doctor_text, function(line)
        if not reading then
            if ceiling == nil and line:match("^ceiling%s*$") then
                ceiling = {}
                reading = true
            end
        elseif line:match("^%s") then
            local key, values = line:match("^%s+([%a_][%w_.]*)%s+(%S.-)%s*$")
            if key == nil then
                error("doctor output: cannot read the ceiling line: " .. line, 0)
            end
            local list = {}
            if values ~= "-" then
                for value in (values .. ", "):gmatch("(.-), ") do
                    list[#list + 1] = value
                end
            end
            ceiling[key] = list
        else
            reading = false
        end
    end)
    if ceiling == nil then
        error("doctor output has no `ceiling` line: " .. doctor_text, 0)
    end
    return ceiling
end

-- Returns true when `list` holds `item`.
local function holds(list, item)
    for _, entry in ipairs(list or {}) do
        if entry == item then
            return true
        end
    end
    return false
end

-- Returns the items of `used` that `allowed` lacks.
local function missing_from(allowed, used)
    local lacking = {}
    for _, item in ipairs(used or {}) do
        if not holds(allowed, item) then
            lacking[#lacking + 1] = item
        end
    end
    return lacking
end

local function row(check, subject, verdict, detail)
    return { check = check, subject = subject, verdict = verdict, detail = detail }
end

-- Check 1: every old recipe is mapped to a target the graph holds, or dropped with a reason.
-- A mapping for a recipe the old build no longer lists is checked too.
local function recipe_rows(input, out)
    local names, seen = {}, {}
    for _, name in ipairs(input.recipes) do
        names[#names + 1] = name
        seen[name] = true
    end
    for name in pairs(input.config.recipes) do
        if not seen[name] then
            names[#names + 1] = name
        end
    end
    table.sort(names)
    for _, name in ipairs(names) do
        local entry = input.config.recipes[name]
        if entry == nil then
            out[#out + 1] = row("1", name, "FAIL", "neither mapped to a target nor dropped")
        elseif entry.dropped ~= nil then
            out[#out + 1] = row("1", name, "PASS", "dropped: " .. entry.dropped)
        elseif input.graph.targets[entry.target] == nil then
            out[#out + 1] = row("1", name, "FAIL", "target " .. entry.target .. " is not in the graph")
        else
            out[#out + 1] = row("1", name, "PASS", "target " .. entry.target)
        end
    end
end

-- Check 2: the old build ran twice and buildl built, each ending in status 0.
local function build_rows(input, out)
    local problems = {}
    local files = {
        { "old-1.build.status", input.status.old1_build },
        { "old-2.build.status", input.status.old2_build },
        { "new.build.status", input.status.new_build },
    }
    for _, file in ipairs(files) do
        if file[2] == nil then
            problems[#problems + 1] = file[1] .. " is missing"
        elseif file[2] ~= 0 then
            problems[#problems + 1] = file[1] .. " is " .. tostring(file[2])
        end
    end
    if #problems == 0 then
        out[#out + 1] = row("2", "builds", "PASS", "both builds ended in status 0")
    else
        out[#out + 1] = row("2", "builds", "FAIL", table.concat(problems, "; "))
    end
end

-- Checks 3, 4 and 4b: one row for each output pair.
local function output_rows(input, out)
    local has_tests = input.config.old.test ~= nil
    for i, pair in ipairs(input.config.outputs) do
        local hashes = input.hashes[i] or {}
        local absent = {}
        for _, where in ipairs({ { "old1", "old-1/" .. pair.old }, { "old2", "old-2/" .. pair.old }, { "new", "new/" .. pair.new } }) do
            if hashes[where[1]] == nil then
                absent[#absent + 1] = where[2]
            end
        end
        if #absent > 0 then
            out[#out + 1] = row("3", pair.old, "FAIL", "missing: " .. table.concat(absent, ", "))
        elseif pair.judge == "tests" then
            if has_tests then
                out[#out + 1] = row("4b", pair.old, "JUDGED BY TESTS", pair.reason)
            else
                out[#out + 1] = row("4b", pair.old, "FAIL", "judged by tests, but the project has no old.test")
            end
        elseif hashes.old1 ~= hashes.old2 then
            if has_tests then
                out[#out + 1] = row("4", pair.old, "NOT REPEATABLE", "old-1 and old-2 differ; the tests decide")
            else
                out[#out + 1] = row("4", pair.old, "FAIL", "old-1 and old-2 differ and the project has no old.test")
            end
        elseif hashes.new ~= hashes.old1 then
            out[#out + 1] = row("3", pair.old, "FAIL", "new/" .. pair.new .. " differs from old-1/" .. pair.old)
        else
            out[#out + 1] = row("3", pair.old, "PASS", "same bytes as new/" .. pair.new)
        end
    end
end

-- Check 5: the old tests and the buildl tests end the same way.
local function test_rows(input, out)
    if input.config.old.test == nil then
        out[#out + 1] = row("5", "tests", "NO TESTS", "the project has no old.test")
        return
    end
    local old, new = input.status.old1_test, input.status.new_test
    if old == nil or new == nil then
        local name = old == nil and "old-1.test.status" or "new.test.status"
        out[#out + 1] = row("5", "tests", "FAIL", name .. " is missing")
    elseif (old == 0) ~= (new == 0) then
        out[#out + 1] = row("5", "tests", "FAIL", "old-1 ended in " .. old .. ", new in " .. new)
    else
        out[#out + 1] = row("5", "tests", "PASS", "both ended in status " .. old)
    end
end

-- Check 6: the ceiling grants no program or variable the old build lacked, and no target
-- reaches the network unless the old build did.
local function permission_rows(input, out)
    local pairs_checked = {
        { "proc.run", input.config.old.programs },
        { "env.read", input.config.old.env },
    }
    for _, entry in ipairs(pairs_checked) do
        local extra = missing_from(entry[2], input.ceiling[entry[1]])
        if #extra == 0 then
            out[#out + 1] = row("6", entry[1], "PASS", "nothing beyond the old build")
        else
            out[#out + 1] = row("6", entry[1], "FAIL", "beyond the old build: " .. table.concat(extra, ", "))
        end
    end
    if input.config.old.network then
        out[#out + 1] = row("6", "network", "PASS", "the old build uses the network")
        return
    end
    local ids = {}
    for id in pairs(input.graph.targets) do
        ids[#ids + 1] = id
    end
    table.sort(ids)
    local reached = false
    for _, id in ipairs(ids) do
        local network = input.graph.targets[id].network
        if network ~= "Sealed" then
            reached = true
            out[#out + 1] = row("6", id, "FAIL", "network is " .. tostring(network) .. ", the old build uses none")
        end
    end
    if not reached then
        out[#out + 1] = row("6", "network", "PASS", "every target is sealed")
    end
end

-- Returns the parity rows, each `{ check, subject, verdict, detail }`, for checks 1 to 6 and
-- 4b. `input` holds: `config` (a validated parity.json), `recipes` (the old build's recipe
-- names), `graph` (the decoded `buildl graph`), `ceiling` (from `M.ceiling`), `hashes` (per
-- `outputs` entry, `{ old1, old2, new }` with `nil` for a missing file) and `status` (exit
-- codes `old1_build`, `old2_build`, `new_build`, `old1_test`, `new_test`; `nil` for a missing
-- status file). Verdicts: `PASS`, `FAIL`, `NOT REPEATABLE`, `JUDGED BY TESTS`, `NO TESTS`.
function M.rows(input)
    local out = {}
    recipe_rows(input, out)
    build_rows(input, out)
    output_rows(input, out)
    test_rows(input, out)
    permission_rows(input, out)
    return out
end

-- Returns `p` as a plain path: `.` and `..` resolved, no trailing slash.
local function plain(p)
    return airsstack.path.normalize(p)
end

-- Raises unless `P` (the project) and `W` (the work folder) are absolute, `W` is not `P` and
-- not below it, and `w_entries` (the names in `W`, `nil` when `W` does not exist) is empty or
-- `nil`.
function M.check_paths(P, W, w_entries)
    if not airsstack.path.is_absolute(P) then
        error("the project folder must be absolute: " .. P, 0)
    end
    if not airsstack.path.is_absolute(W) then
        error("the work folder must be absolute: " .. W, 0)
    end
    local project, work = plain(P), plain(W)
    if work == project or work:sub(1, #project + 1) == project .. "/" then
        error("the work folder must be outside the project: " .. W .. " is in " .. P, 0)
    end
    if w_entries ~= nil and #w_entries > 0 then
        error("work folder " .. W .. " is not empty", 0)
    end
end

-- Returns the set `{ [path] = true }` of the paths `prepare` does not copy: `.git`, `.buildl`,
-- `out`, `parity.json`, every `skip` entry and the `old` and `new` path of every output.
function M.left_out(config)
    local left = { [".git"] = true, [".buildl"] = true, out = true, ["parity.json"] = true }
    for _, path in ipairs(config.skip or {}) do
        left[plain(path)] = true
    end
    for _, pair in ipairs(config.outputs) do
        left[plain(pair.old)] = true
        left[plain(pair.new)] = true
    end
    return left
end

-- Returns true when `rel`, a path relative to the project, is a left-out path or lies below one.
function M.is_left_out(rel, left)
    local path = plain(rel)
    while path ~= "." and path ~= "" do
        if left[path] then
            return true
        end
        local parent = path:match("^(.*)/[^/]+$")
        if parent == nil then
            return false
        end
        path = parent
    end
    return false
end

return M
