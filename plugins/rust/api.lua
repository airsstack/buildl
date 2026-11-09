-- The public table of the plugin: the functions that declare targets and the helpers.
local graph = require("graph")
local options = require("options")

local M = {}

-- Runs `work`; if it raises, raises the message again from the caller of the function that
-- called `relocate`, with the plugin's own location taken off, so that the location is the
-- build file's call. `prefix` is the start of the message the plugin's modules write, such as
-- "rust.binary(". An error that does not hold it, such as one of `buildl`'s own, goes on as it
-- was. Only the public functions call it, and never as a tail call (`return relocate(...)`),
-- which would drop the frame the error level counts.
local function relocate(prefix, work)
    local ok, err = pcall(work)
    if ok then
        return
    end
    err = tostring(err)
    local start = err:find(prefix, 1, true)
    if start == nil then
        error(err, 0)
    end
    error(err:sub(start), 3)
end

-- Returns "macos" for a target triple holding `apple-darwin` and "linux" for one holding
-- `linux`. Raises, naming `rust.platform` and `triple`, for any other.
local function platform_of(triple)
    if triple:find("apple-darwin", 1, true) then
        return "macos"
    elseif triple:find("linux", 1, true) then
        return "linux"
    end
    error(string.format("rust.platform: the target triple %s is neither macos nor linux", triple), 0)
end

-- Returns the public table over the `buildl` table the plugin is given. The state of
-- `rust.platform` belongs to the table returned, not to the module.
function M.new(buildl)
    local rust = {}

    -- Returns whether `given` names a library whose `kind` lists "cdylib": the file cargo writes
    -- for it is named after the platform.
    local function shared(kind, given)
        if kind ~= "library" or type(given) ~= "table" or type(given.kind) ~= "table" then
            return false
        end
        for _, item in pairs(given.kind) do
            if item == "cdylib" then
                return true
            end
        end
        return false
    end

    -- Returns the function that declares the targets of `rust.<kind>(name, options)`. A library
    -- holding a cdylib asks `rust.platform` first, so its file is named as cargo names it.
    local function declarer(kind)
        return function(name, given)
            local platform = shared(kind, given) and rust.platform() or nil
            relocate("rust." .. kind .. "(", function()
                graph.declare(buildl, kind, name, options.check(kind, name, given, platform))
            end)
        end
    end

    rust.binary = declarer("binary")
    rust.library = declarer("library")
    rust.test = declarer("test")

    -- Returns `{ "-p", name }`. Raises, naming `rust.package`, at the caller for a `name` that
    -- is not a string.
    function rust.package(name)
        if type(name) ~= "string" then
            error("rust.package: the name must be a string", 2)
        end
        return { "-p", name }
    end

    -- Returns `{ "--features", "<list joined with commas>" }`, or `{}` for an empty list.
    -- Raises, naming `rust.features`, at the caller for anything but a list of strings.
    function rust.features(list)
        if not options.is_list(list) then
            error("rust.features: the features must be a list of strings", 2)
        end
        if #list == 0 then
            return {}
        end
        return { "--features", table.concat(list, ",") }
    end

    -- Returns `lib<name>.so` for "linux", `lib<name>.dylib` for "macos", nil for nil. Raises,
    -- naming `rust.shared_name`, at the caller for a `name` that is not a string.
    function rust.shared_name(name, platform)
        if type(name) ~= "string" then
            error("rust.shared_name: the name must be a string", 2)
        end
        return options.shared_name(name, platform)
    end

    local declared = false

    -- Returns "linux" or "macos" once the discovery of `:rust.platform` has run, else nil.
    -- The first call declares that discovery; a later call declares nothing.
    function rust.platform()
        if not declared then
            declared = true
            buildl.discover("rust.platform", { run = { "rustc", "-vV" }, stdout = true })
        end
        local output = buildl.discovered(":rust.platform")
        if output == nil then
            return nil
        end
        local result
        relocate("rust.platform", function()
            local triple = output:match("host:%s*(%S+)")
            if triple == nil then
                error("rust.platform: the output of rustc -vV has no host line", 0)
            end
            result = platform_of(triple)
        end)
        return result
    end

    return rust
end

return M
