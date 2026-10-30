-- Builds the compiler and linker flags that differ by folder, by define and by platform.
-- Pure: it uses no `buildl`; the folder is passed in.
local M = {}

-- Raises, naming `fname` and `platform`, unless `platform` is `nil`, "linux" or "macos". The
-- error is located at the caller of the public function `fname`.
local function check_platform(fname, platform)
    if platform ~= nil and platform ~= "linux" and platform ~= "macos" then
        error(string.format("%s: the platform %s is neither linux nor macos", fname, tostring(platform)), 3)
    end
end

-- Returns `{ "-I<folder>/<dir>", ... }` for each of `dirs`, in order; at the root (`folder` of
-- "") each flag is `-I<dir>`.
function M.includes(folder, dirs)
    local flags = {}
    for _, dir in ipairs(dirs) do
        if folder == "" then
            flags[#flags + 1] = "-I" .. dir
        else
            flags[#flags + 1] = "-I" .. folder .. "/" .. dir
        end
    end
    return flags
end

-- Returns the flags of `map`, sorted by key: `-D<key>` for `true`, nothing for `false`,
-- `-D<key>=<value>` for a string or a number. Raises, naming `c.defines` and the key, for a key
-- that is not a string and for any other value.
function M.defines(map)
    local keys = {}
    for key in pairs(map) do
        if type(key) ~= "string" then
            error(string.format("c.defines: the key %s is a %s, not a string", tostring(key), type(key)), 2)
        end
        keys[#keys + 1] = key
    end
    table.sort(keys)
    local flags = {}
    for _, key in ipairs(keys) do
        local value = map[key]
        local kind = type(value)
        if value == true then
            flags[#flags + 1] = "-D" .. key
        elseif kind == "string" or kind == "number" then
            flags[#flags + 1] = "-D" .. key .. "=" .. tostring(value)
        elseif value ~= false then
            error(string.format("c.defines: the define %s is a %s, not a boolean, string or number", key, kind), 2)
        end
    end
    return flags
end

-- Returns `lib<name>.so` for "linux" and `lib<name>.dylib` for "macos"; `nil` when `platform` is
-- `nil`. Raises, naming `c.shared_name` and `platform`, for any other.
function M.shared_name(name, platform)
    check_platform("c.shared_name", platform)
    if platform == "linux" then
        return "lib" .. name .. ".so"
    elseif platform == "macos" then
        return "lib" .. name .. ".dylib"
    end
    return nil
end

-- Returns the flags that give a shared library `file` as its runtime name: `{}` when `platform`
-- is `nil`. Raises, naming `c.soname` and `platform`, for any other than "linux" and "macos".
function M.soname(file, platform)
    check_platform("c.soname", platform)
    if platform == "linux" then
        return { "-Wl,-soname," .. file }
    elseif platform == "macos" then
        return { "-install_name", "@rpath/" .. file }
    end
    return {}
end

-- Returns the flags that make a binary look for shared libraries in its own folder: `{}` when
-- `platform` is `nil`. Raises, naming `c.rpath` and `platform`, for any other than "linux" and
-- "macos".
function M.rpath(platform)
    check_platform("c.rpath", platform)
    if platform == "linux" then
        return { "-Wl,-rpath,$ORIGIN" }
    elseif platform == "macos" then
        return { "-Wl,-rpath,@loader_path" }
    end
    return {}
end

-- Returns "macos" for a target triple holding `apple-darwin` and "linux" for one holding
-- `linux`. Raises, naming `c.platform` and `triple`, for any other.
function M.platform_of(triple)
    if triple:find("apple-darwin", 1, true) then
        return "macos"
    elseif triple:find("linux", 1, true) then
        return "linux"
    end
    error(string.format("c.platform: the target triple %s is neither macos nor linux", triple), 2)
end

return M
