-- Declares the targets of one call of the plugin through the `buildl` table it is given: a
-- discovery target per source, then, once every one has run, an object per source and the last
-- step. Pure apart from that table.
local depfile = require("depfile")

local M = {}

-- Raises with the message prefixed by the function and the target name, located at the caller
-- of `declare`. Only `declare` itself calls it, so the level is fixed.
local function fail(kind, name, message)
    error(string.format("c.%s(%s): %s", kind, name, message), 3)
end

-- Returns the copy of the list `value`.
local function copy(value)
    local out = {}
    for i, item in ipairs(value) do
        out[i] = item
    end
    return out
end

-- Returns the list of the items of the lists and strings in the arguments, in order.
local function join(...)
    local out = {}
    for _, part in ipairs({ ... }) do
        if type(part) == "table" then
            for _, item in ipairs(part) do
                out[#out + 1] = item
            end
        else
            out[#out + 1] = part
        end
    end
    return out
end

-- Returns the label of the target `name` in this build file.
local function label(name)
    return ":" .. name
end

-- Returns `path` from the workspace root, given the build file's `folder`.
local function from_root(folder, path)
    if folder == "" then
        return path
    end
    return folder .. "/" .. path
end

-- Returns the set of `headers`, each normalised where that is possible.
local function header_set(headers)
    local set = {}
    for _, header in ipairs(headers) do
        local ok, normal = pcall(depfile.normalise, header)
        set[ok and normal or header] = true
    end
    return set
end

-- Returns the inputs of the object of `source` that its discovery `text` calls for, in the
-- order read: the source, then each header under `folder` that is in the set `headers`. A
-- prerequisite outside that set is dropped when `has_header_deps`, since it is then an output of
-- one of them. Returns nil and a message when there is no `obj` rule, a prerequisite is absolute
-- or climbs above the workspace root, or one cannot be placed and there are no `header_deps`.
local function object_inputs(text, source, folder, headers, has_header_deps)
    local rule
    for _, candidate in ipairs(depfile.parse(text)) do
        if candidate.target == "obj" then
            rule = candidate
            break
        end
    end
    if rule == nil then
        return nil, string.format("the discovery of %s holds no rule for obj", source)
    end

    local inputs = { source }
    local seen = {}
    local prefix = folder == "" and "" or folder .. "/"
    for i = 2, #rule.prerequisites do
        local path = rule.prerequisites[i]
        if path:sub(1, 1) == "/" then
            return nil,
                string.format(
                    "the header %s, read by %s, is outside the workspace; a header there belongs in an -isystem folder, whose headers -MM leaves out",
                    path,
                    source
                )
        end
        local ok, normal = pcall(depfile.normalise, path)
        if not ok then
            return nil, string.format("the header %s, read by %s, climbs above the workspace root", path, source)
        end
        local inside = normal:sub(1, #prefix) == prefix and normal:sub(#prefix + 1) or nil
        if inside ~= nil and headers[inside] then
            if not seen[inside] then
                seen[inside] = true
                inputs[#inputs + 1] = inside
            end
        elseif not has_header_deps then
            return nil,
                string.format(
                    "the header %s, read by %s, is %s; list it in headers, or reach it through header_deps (one build file per project)",
                    normal,
                    source,
                    folder == "" and "not among headers" or "outside " .. folder .. "/ or not among headers"
                )
        end
    end
    return inputs
end

-- Declares the targets of one call. `kind` is "binary", "library", "shared_library" or "test",
-- `name` the target name and `options` the table `options.check` returned for the call. Before
-- discovery has run it declares the version probes, when asked for, and one discovery target per
-- source; once every source has a result it declares the objects and the last step too, and
-- raises, naming the function, the target and the header or source, for a result that cannot be
-- placed.
function M.declare(buildl, kind, name, options)
    local folder = buildl.folder()
    local languages = {}
    for _, source in ipairs(options.srcs) do
        languages[source.language] = true
    end

    local probes = {}
    if options.version_probe then
        local compilers = { c = options.cc, cpp = options.cxx }
        for _, language in ipairs({ "c", "cpp" }) do
            if languages[language] then
                probes[language] = name .. ".version." .. language
                buildl.target(probes[language], {
                    run = { compilers[language], "--version" },
                    stdout = true,
                    always = true,
                })
            end
        end
    end

    local entries = {}
    local complete = true
    for i, source in ipairs(options.srcs) do
        local stem = name .. "." .. (source.path:gsub("/", "."))
        local program, flags = options.cc, options.cflags
        if source.language == "cpp" then
            program, flags = options.cxx, options.cxxflags
        end
        local root = from_root(folder, source.path)
        local discovery = stem .. ".deps"
        buildl.discover(discovery, {
            run = join(program, flags, { "-MM", "-MT", "obj", root }),
            inputs = join(source.path, options.headers),
            deps = copy(options.header_deps),
            stdout = true,
        })
        local text = buildl.discovered(label(discovery))
        complete = complete and text ~= nil
        entries[i] = { stem = stem, source = source, program = program, flags = flags, root = root, text = text }
    end
    if not complete then
        return
    end

    local headers = header_set(options.headers)
    local objects = {}
    for i, entry in ipairs(entries) do
        local inputs, message =
            object_inputs(entry.text, entry.source.path, folder, headers, #options.header_deps > 0)
        if inputs == nil then
            fail(kind, name, message)
        end
        local object = entry.stem .. ".o"
        local probe = probes[entry.source.language]
        buildl.target(object, {
            run = join(entry.program, entry.flags, { "-c", "-o", "$out", entry.root }),
            inputs = inputs,
            deps = join(options.header_deps, probe and label(probe) or {}),
        })
        objects[i] = label(object)
    end

    if kind == "library" then
        -- macOS `ar` stamps the archive with the time it ran; ZERO_AR_DATE keeps the bytes the
        -- same from run to run. GNU `ar` ignores it and is deterministic already.
        buildl.target(name, {
            run = { "sh", "-c", 'ZERO_AR_DATE=1 exec ar rcs "$0" "$@"', "$out", "$deps" },
            deps = objects,
            outputs = { options.output },
            env = { "PATH" },
        })
        return
    end

    local link = kind == "test" and name .. ".bin" or name
    local shared = kind == "shared_library" and { "-shared" } or {}
    buildl.target(link, {
        run = join(options.linker, options.ldflags, shared, { "-o", "$out", "$deps" }, options.ldlibs),
        deps = join(objects, options.deps),
        env = { "PATH" },
        outputs = { options.output },
    })
    if kind ~= "test" then
        return
    end

    if #options.deps == 0 then
        buildl.test(name, { run = { "$deps" }, deps = { label(link) } })
    else
        buildl.test(name, {
            run = { "sh", "-c", 'exec "$0"', "$deps" },
            deps = join(label(link), options.deps),
        })
    end
end

return M
