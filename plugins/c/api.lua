-- The public table of the plugin: the functions that declare targets and the helpers.
local depfile = require("depfile")
local flags = require("flags")
local graph = require("graph")
local options = require("options")

local M = {}

-- Runs `work`; if it raises, raises the message again from the caller of the function that
-- called `relocate`, with the plugin's own location taken off, so that the location is the
-- build file's call. `prefix` is the start of the message the plugin's modules write, such as
-- "c.binary(". An error that does not hold it, such as one of `buildl`'s own, goes on as it was.
-- Only the public functions call it, and not as their last call.
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

-- Returns the public table over the `buildl` table the plugin is given. The state of
-- `c.platform` belongs to the table returned, not to the module.
function M.new(buildl)
    local c = {}

    -- Returns the function that declares the targets of `c.<kind>(name, options)`.
    local function declarer(kind)
        return function(name, given)
            relocate("c." .. kind .. "(", function()
                graph.declare(buildl, kind, name, options.check(kind, name, given))
            end)
        end
    end

    c.binary = declarer("binary")
    c.library = declarer("library")
    c.shared_library = declarer("shared_library")
    c.test = declarer("test")

    -- Returns the `-I` flags of `dirs`, a list of strings, under the build file's folder. Raises,
    -- naming `c.includes`, at the caller for anything else.
    function c.includes(dirs)
        if type(dirs) ~= "table" then
            error("c.includes: the directories must be a list of strings", 2)
        end
        for i, dir in ipairs(dirs) do
            if type(dir) ~= "string" then
                error(string.format("c.includes: the directory %d is a %s, not a string", i, type(dir)), 2)
            end
        end
        -- Through a local, so the call is not a tail call and the caller's frame stays.
        local result = flags.includes(buildl.folder(), dirs)
        return result
    end

    c.defines = flags.defines
    c.shared_name = flags.shared_name
    c.soname = flags.soname
    c.rpath = flags.rpath
    c.depfile = { parse = depfile.parse }

    local platform_cc

    -- Returns "linux" or "macos" once the discovery of `:c.platform` has run, else nil. The
    -- first call declares that discovery, with the compiler `given.cc` (default "cc"); a later
    -- call that names another compiler raises.
    function c.platform(given)
        given = given or {}
        relocate("c.platform", function()
            if type(given) ~= "table" then
                error("c.platform: the options must be a table", 0)
            end
            for key, value in pairs(given) do
                if key ~= "cc" then
                    error(string.format("c.platform: the option %s is not known", tostring(key)), 0)
                end
                if type(value) ~= "string" then
                    error("c.platform: the option cc must be a string", 0)
                end
            end
            local cc = given.cc or "cc"
            if platform_cc == nil then
                platform_cc = cc
                buildl.discover("c.platform", { run = { cc, "-dumpmachine" }, stdout = true })
            elseif platform_cc ~= cc then
                error(
                    string.format("c.platform: the compiler %s differs from %s, which the first call named", cc, platform_cc),
                    0
                )
            end
        end)
        local triple = buildl.discovered(":c.platform")
        if triple == nil then
            return nil
        end
        local result
        relocate("c.platform", function()
            result = flags.platform_of(triple)
        end)
        return result
    end

    return c
end

return M
