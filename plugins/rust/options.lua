-- Checks and fills the options of one call of the plugin, and holds the list check the helpers
-- of the public table share.
-- Pure: it uses no `buildl`.
local M = {}

-- The type of each option: "list" is a table of strings, "string", "boolean".
local TYPES = {
    args = "list",
    deps = "list",
    profile = "string",
    output = "string",
    kind = "string",
    version_probe = "boolean",
}

-- The option names in order, so the type check names the same option on every run.
local NAMES = {}
for key in pairs(TYPES) do
    NAMES[#NAMES + 1] = key
end
table.sort(NAMES)

-- The folder under `target/` that cargo writes the output of a profile to; a profile that is
-- not listed writes to a folder of its own name.
local PROFILE_DIRS = { dev = "debug", test = "debug", release = "release", bench = "release" }

-- Raises with the message prefixed by the function and the target name, located at the caller
-- of `check`.
local function fail(kind, name, message)
    error(string.format("rust.%s(%s): %s", kind, name, message), 3)
end

-- Returns whether `value` is a table whose keys are 1 to n and whose items are strings. Also
-- exported as `is_list`, so the helpers of the public table check lists the same way.
local function is_list(value)
    if type(value) ~= "table" then
        return false
    end
    local count = 0
    for _, item in pairs(value) do
        if type(item) ~= "string" then
            return false
        end
        count = count + 1
    end
    return count == #value
end

M.is_list = is_list

-- Returns the copy of the list `value`.
local function copy(value)
    local out = {}
    for i, item in ipairs(value) do
        out[i] = item
    end
    return out
end

-- Returns the options of a call to `rust.<kind>(name, options)`, checked and filled: `args`
-- and `deps` are lists, `profile` defaults to "dev", `profile_dir` is the folder under
-- `target/` that profile writes to, `version_probe` is a boolean, `kind` is set for a library
-- only, and `output` is set except for a test. `options` may be nil. Raises, naming the
-- option, for one that is unknown, of the wrong type or refused by the kind (`kind` outside a
-- library, `output` for a test), and for a library whose `kind` is not "staticlib" or "cdylib".
function M.check(kind, name, options)
    if kind ~= "binary" and kind ~= "library" and kind ~= "test" then
        error(
            string.format("rust.%s(%s): the kind is not binary, library or test", tostring(kind), tostring(name)),
            2
        )
    end
    if options == nil then
        options = {}
    end
    if type(options) ~= "table" then
        fail(kind, name, "the options must be a table")
    end

    local unknown = {}
    for key in pairs(options) do
        if TYPES[key] == nil then
            unknown[#unknown + 1] = tostring(key)
        end
    end
    if #unknown > 0 then
        table.sort(unknown)
        fail(kind, name, string.format("the option %s is not known", unknown[1]))
    end

    if kind ~= "library" and options.kind ~= nil then
        fail(kind, name, "the option kind is refused")
    end

    if kind == "test" and options.output ~= nil then
        fail(kind, name, "the option output is refused")
    end

    for _, key in ipairs(NAMES) do
        local expected = TYPES[key]
        local value = options[key]
        if value ~= nil then
            local good
            if expected == "list" then
                good = is_list(value)
            else
                good = type(value) == expected
            end
            if not good then
                local what = expected == "list" and "a list of strings" or "a " .. expected
                fail(kind, name, string.format("the option %s must be %s", key, what))
            end
        end
    end

    if kind == "library" and options.kind ~= "staticlib" and options.kind ~= "cdylib" then
        fail(kind, name, 'kind must be "staticlib" or "cdylib"')
    end

    local profile = options.profile or "dev"
    local output
    if kind == "binary" then
        output = options.output or name
    elseif kind == "library" then
        output = options.output or (options.kind == "staticlib" and "lib" .. name .. ".a" or name)
    end
    return {
        args = copy(options.args or {}),
        deps = copy(options.deps or {}),
        profile = profile,
        profile_dir = PROFILE_DIRS[profile] or profile,
        output = output,
        kind = kind == "library" and options.kind or nil,
        version_probe = options.version_probe or false,
    }
end

return M
