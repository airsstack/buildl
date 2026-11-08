-- Declares the targets of one call of the plugin through the `buildl` table it is given: a
-- target that runs `cargo build` and copies the file cargo wrote to `$out`, or a test that runs
-- `cargo test`, each in the build file's folder. Pure apart from that table.
local M = {}

-- Raises with the message prefixed by the function and the target name, located at the caller
-- of `declare`. Only `declare` itself calls it, so the level is fixed.
local function fail(kind, name, message)
    error(string.format("rust.%s(%s): %s", kind, name, message), 3)
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

-- Returns `word` as one shell word: as it is when it holds only letters, digits and `._/-`,
-- else in single quotes.
local function quote(word)
    if word:match("^[%w._/-]+$") then
        return word
    end
    return "'" .. word:gsub("'", [['\'']]) .. "'"
end

-- Returns `path` from the workspace root, given the build file's `folder`.
local function from_root(folder, path)
    if folder == "" then
        return path
    end
    return folder .. "/" .. path
end

-- Returns the files `listed` that are not under a `target/` folder.
local function without_target(listed)
    local kept = {}
    for _, file in ipairs(listed) do
        if file:sub(1, 7) ~= "target/" then
            kept[#kept + 1] = file
        end
    end
    return kept
end

-- Returns whether `files` holds `file`.
local function has(files, file)
    for _, entry in ipairs(files) do
        if entry == file then
            return true
        end
    end
    return false
end

-- Returns the shell script of one cargo `verb` ("build --locked --bin x", "test --locked") run
-- in `folder`: cargo with the plugin's own arguments, then the caller's, which the shell hands
-- over as `"$@"`.
local function cargo(folder, verb, profile)
    local command = string.format("cargo %s --profile %s --target-dir target \"$@\"", verb, quote(profile))
    if folder == "" then
        return command
    end
    return string.format("(cd %s && %s)", quote(folder), command)
end

-- Returns the shell command that copies the file cargo wrote for `kind` ("binary" or the crate
-- type of a library) and `name` under `profile_dir` to "$0", the output. A cdylib is the
-- `.so` file when cargo wrote one, else the `.dylib`.
local function copy(folder, kind, name, profile_dir)
    local built = from_root(folder, "target/" .. profile_dir)
    local function file(leaf)
        return quote(built .. "/" .. leaf)
    end
    if kind == "cdylib" then
        return string.format(
            'if [ -f %s ]; then cp %s "$0"; else cp %s "$0"; fi',
            file("lib" .. name .. ".so"),
            file("lib" .. name .. ".so"),
            file("lib" .. name .. ".dylib")
        )
    end
    local leaf = kind == "staticlib" and "lib" .. name .. ".a" or name
    return string.format('cp %s "$0"', file(leaf))
end

-- Declares the targets of one call. `kind` is "binary", "library" or "test", `name` the target
-- name and `options` the table `options.check` returned for the call. Raises, naming the
-- function and the target, when the build file's folder holds no `Cargo.toml`, or no
-- `Cargo.lock`, before declaring anything. With `version_probe` it declares `<name>.version` and
-- makes the target depend on it.
function M.declare(buildl, kind, name, options)
    local folder = buildl.folder()
    local listed = buildl.sources("**/*")
    if not has(listed, "Cargo.toml") then
        fail(kind, name, "no Cargo.toml in " .. (folder == "" and "." or folder))
    end
    if not has(listed, "Cargo.lock") then
        fail(kind, name, "no Cargo.lock beside Cargo.toml; run cargo generate-lockfile")
    end

    local deps = join(options.deps)
    if options.version_probe then
        buildl.target(name .. ".version", { run = { "rustc", "-vV" }, stdout = true, always = true })
        deps[#deps + 1] = ":" .. name .. ".version"
    end

    local declaration = {
        inputs = without_target(listed),
        deps = deps,
        env = { HOME = "unkeyed", PATH = "unkeyed" },
        network = true,
    }

    if kind == "test" then
        -- "sh" fills `$0`, so `"$@"` holds exactly the caller's arguments.
        declaration.run = join(
            { "sh", "-c", cargo(folder, "test --locked", options.profile), "sh" },
            options.args
        )
        buildl.test(name, declaration)
        return
    end

    local verb = kind == "library" and "build --locked --lib" or "build --locked --bin " .. quote(name)
    local script = cargo(folder, verb, options.profile)
        .. " && "
        .. copy(folder, kind == "library" and options.kind or "binary", name, options.profile_dir)
    declaration.run = join({ "sh", "-c", script, "$out" }, options.args)
    declaration.outputs = { options.output }
    buildl.target(name, declaration)
end

return M
