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
    kind = "list",
    version_probe = "boolean",
}

-- The crate types `rust.library` copies, as Cargo.toml's `crate-type` names them.
local LIBRARY_KINDS = { staticlib = true, cdylib = true }

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

-- Returns `lib<name>.so` for "linux", `lib<name>.dylib` for "macos", nil for any other
-- `platform`, nil included: the file cargo writes for a `cdylib`.
function M.shared_name(name, platform)
    if platform == "linux" then
        return "lib" .. name .. ".so"
    elseif platform == "macos" then
        return "lib" .. name .. ".dylib"
    end
    return nil
end

-- Returns the files of a library, one `{ kind, output }` per item of `kinds`, in its order:
-- `lib<name>.a` for a staticlib, and for a cdylib the file cargo writes on `platform`, or
-- `name` while `platform` is nil.
local function library_files(name, kinds, platform)
    local files = {}
    for i, kind in ipairs(kinds) do
        local output
        if kind == "staticlib" then
            output = "lib" .. name .. ".a"
        else
            output = M.shared_name(name, platform) or name
        end
        files[i] = { kind = kind, output = output }
    end
    return files
end

-- Returns the options of a call to `rust.<kind>(name, options)`, checked and filled: `args`
-- and `deps` are lists, `profile` defaults to "dev", `profile_dir` is the folder under
-- `target/` that profile writes to, `version_probe` is a boolean, `output` is set for a binary
-- only, and `libraries` for a library only: one `{ kind, output }` per crate type of `kind`,
-- named as cargo names the file on `platform` (see `library_files`). `options` and `platform`
-- may be nil. Raises, naming the option, for one that is unknown, of the wrong type or refused
-- by the kind (`kind` outside a library, `output` for a library or a test), and for a library
-- whose `kind` is not a list of "staticlib" and "cdylib", each at most once.
function M.check(kind, name, options, platform)
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

    if kind ~= "binary" and options.output ~= nil then
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

    local libraries
    if kind == "library" then
        local kinds = options.kind
        if kinds == nil or #kinds == 0 then
            fail(kind, name, 'kind must list "staticlib", "cdylib" or both')
        end
        local seen = {}
        for _, item in ipairs(kinds) do
            if not LIBRARY_KINDS[item] then
                fail(kind, name, 'kind must list "staticlib", "cdylib" or both')
            end
            if seen[item] then
                fail(kind, name, string.format("kind lists %s twice", item))
            end
            seen[item] = true
        end
        libraries = library_files(name, kinds, platform)
    end

    local profile = options.profile or "dev"
    return {
        args = copy(options.args or {}),
        deps = copy(options.deps or {}),
        profile = profile,
        profile_dir = PROFILE_DIRS[profile] or profile,
        output = kind == "binary" and (options.output or name) or nil,
        libraries = libraries,
        version_probe = options.version_probe or false,
    }
end

return M
