-- Checks and fills the options of one call of the plugin.
-- Pure: it uses no `buildl`.
local M = {}

-- The type of each option: "list" is a table of strings, "string", "boolean".
local TYPES = {
    srcs = "list",
    headers = "list",
    header_deps = "list",
    cflags = "list",
    cxxflags = "list",
    ldflags = "list",
    ldlibs = "list",
    deps = "list",
    cc = "string",
    cxx = "string",
    linker = "string",
    output = "string",
    version_probe = "boolean",
}

-- The option names in order, so the type check names the same option on every run.
local NAMES = {}
for key in pairs(TYPES) do
    NAMES[#NAMES + 1] = key
end
table.sort(NAMES)

-- The options that fall back to an empty list.
local LISTS = { "headers", "header_deps", "cflags", "cxxflags", "ldflags", "ldlibs", "deps" }

-- The options an archive does not take: it is not linked.
local LINK_ONLY = { "ldflags", "ldlibs", "linker", "deps" }

-- The language of each source extension.
local LANGUAGES = { c = "c", cc = "cpp", cpp = "cpp", cxx = "cpp" }

-- Raises with the message prefixed by the function and the target name, located at the caller
-- of `check`.
local function fail(kind, name, message)
    error(string.format("c.%s(%s): %s", kind, name, message), 3)
end

-- Returns whether `value` is a table whose keys are 1 to n and whose items are strings.
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

-- Returns the copy of the list `value`.
local function copy(value)
    local out = {}
    for i, item in ipairs(value) do
        out[i] = item
    end
    return out
end

-- Returns the language of `path` by its extension, or nil for one that is not a source's.
local function language_of(path)
    local extension = path:match("[^/]%.([^./]+)$")
    return extension and LANGUAGES[extension]
end

-- Returns the options of a call to `c.<kind>(name, options)`, checked and filled:
-- `srcs` is a list of `{ path, language }` with language "c" or "cpp", `linker` is set
-- except for a library, and every other option is set. Raises, naming the option or the
-- source, for an option that is unknown, of the wrong type or refused by the kind, a missing
-- or empty `srcs`, and a source whose extension is not `.c`, `.cc`, `.cpp` or `.cxx`.
function M.check(kind, name, options)
    if kind ~= "binary" and kind ~= "library" and kind ~= "shared_library" and kind ~= "test" then
        error(
            string.format("c.%s(%s): the kind is not binary, library, shared_library or test", tostring(kind), tostring(name)),
            2
        )
    end
    if type(options) ~= "table" then
        fail(kind, name, "the options are missing; they must be a table")
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

    if kind == "library" then
        for _, key in ipairs(LINK_ONLY) do
            if options[key] ~= nil then
                fail(kind, name, string.format("the option %s is refused: an archive is not linked", key))
            end
        end
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

    if options.srcs == nil or #options.srcs == 0 then
        fail(kind, name, "the option srcs is required and must not be empty")
    end

    local filled = {
        cc = options.cc or "cc",
        cxx = options.cxx or "c++",
        version_probe = options.version_probe or false,
        srcs = {},
    }
    for _, key in ipairs(LISTS) do
        filled[key] = copy(options[key] or {})
    end

    local cpp = false
    for i, path in ipairs(options.srcs) do
        local language = language_of(path)
        if language == nil then
            fail(kind, name, string.format("the source %s is not a .c, .cc, .cpp or .cxx file", path))
        end
        cpp = cpp or language == "cpp"
        filled.srcs[i] = { path = path, language = language }
    end

    if kind ~= "library" then
        filled.linker = options.linker or (cpp and filled.cxx or filled.cc)
    end
    filled.output = options.output or (kind == "test" and name .. ".bin" or name)
    return filled
end

return M
