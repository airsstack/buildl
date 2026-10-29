-- The parity check's entry. Run as a script: `parity.lua prepare P W` copies the project into
-- `W`; `parity.lua compare P W` prints the parity table and fails when a row is `FAIL`. `P` is
-- the project folder and `W` a work folder; `prepare` needs both absolute. All judging is in
-- `parity_lib`; this file does the reading, hashing and copying.
--
-- Loaded with `require`, it returns its functions and runs nothing.
local lib = require("parity_lib")

local fs, path, stdio = airsstack.fs, airsstack.path, airsstack.stdio

local M = {}

-- Reads and validates `P/parity.json`.
local function read_config(P)
    local file = path.join(P, "parity.json")
    if not fs.is_file(file) then
        error("no parity.json in the project folder " .. P, 0)
    end
    return lib.config(airsstack.json.decode(fs.read(file)))
end

-- Copies the project into `W/old-1`, `W/old-2` and `W/new`, leaving out what `parity.json`
-- and `lib.left_out` name, and returns the three folders. Writes only below `W`.
--
-- The text of `P` and `W` cannot show a symlink, so once `W` exists the real paths of both are
-- compared. The project is listed first, so nothing of `W` is ever read as part of it, and a
-- `W` this call created is removed again when it turns out to be inside `P`.
function M.prepare(P, W)
    lib.check_paths(P, W, nil)
    local config = read_config(P)
    lib.check_paths(P, W, fs.exists(W) and fs.list(W) or nil)

    local entries = fs.walk(P)
    local created = not fs.exists(W)
    fs.mkdir(W)
    local outside, err = pcall(lib.check_outside, fs.canonicalize(P), fs.canonicalize(W))
    if not outside then
        if created then
            fs.remove_dir(W)
        end
        error(err, 0)
    end

    local left = lib.left_out(config)
    local copies = { path.join(W, "old-1"), path.join(W, "old-2"), path.join(W, "new") }
    for _, copy in ipairs(copies) do
        fs.mkdir(copy)
    end
    for _, rel in ipairs(entries) do
        if not lib.is_left_out(rel, left) then
            local from = path.join(P, rel)
            for _, copy in ipairs(copies) do
                local to = path.join(copy, rel)
                if fs.is_dir(from) then
                    fs.mkdir(to)
                else
                    fs.mkdir(path.dirname(to))
                    fs.copy(from, to)
                end
            end
        end
    end
    return copies
end

-- The files `lib.recipes` reads for `config.tool`, as `{ [path relative to P] = text }`.
-- For `make` that is each of make's own file names present; `lib.recipes` takes the first in
-- make's order. For `shell` it is every `*.sh` outside the left-out paths, none being possible.
-- For the other tools it raises when the old build's file is not there.
local function old_build_files(P, config)
    local files = {}
    local names = {
        make = { "GNUmakefile", "makefile", "Makefile" },
        just = { "justfile" },
        npm = { "package.json" },
    }
    if config.tool == "shell" then
        local left = lib.left_out(config)
        for _, rel in ipairs(fs.walk(P)) do
            if rel:match("%.sh$") and not lib.is_left_out(rel, left) and fs.is_file(path.join(P, rel)) then
                files[rel] = fs.read(path.join(P, rel))
            end
        end
        return files
    end
    for _, name in ipairs(names[config.tool]) do
        if fs.is_file(path.join(P, name)) then
            files[name] = fs.read(path.join(P, name))
        end
    end
    if next(files) == nil then
        error("no " .. table.concat(names[config.tool], ", ") .. " in the project folder " .. P, 0)
    end
    return files
end

-- The exit code a status file holds; nil when the file is missing, false when it holds no
-- number.
local function read_status(W, name)
    local file = path.join(W, name)
    if not fs.is_file(file) then
        return nil
    end
    return tonumber((fs.read(file):gsub("%s+", ""))) or false
end

-- The SHA-256 of `file`, or nil when it is not a file.
local function hash_of(file)
    if fs.is_file(file) then
        return airsstack.hash.hash_file(file)
    end
    return nil
end

-- Judges the old build against the buildl build from the files `prepare` and step 2 left in
-- `W`. Reads `P` and `W`; writes nothing and runs nothing. Returns
-- `{ table = <rendered text>, passed = <no FAIL row>, rows = <the rows> }`.
function M.compare(P, W)
    local config = read_config(P)
    local hashes = {}
    for i, pair in ipairs(config.outputs) do
        hashes[i] = {
            old1 = hash_of(path.join(W, "old-1", pair.old)),
            old2 = hash_of(path.join(W, "old-2", pair.old)),
            new = hash_of(path.join(W, "new", pair.new)),
        }
    end
    local rows = lib.rows({
        config = config,
        recipes = lib.recipes(config.tool, old_build_files(P, config)),
        graph = airsstack.json.decode(fs.read(path.join(W, "new.graph.json"))),
        ceiling = lib.ceiling(fs.read(path.join(W, "new.doctor.txt"))),
        hashes = hashes,
        status = {
            old1_build = read_status(W, "old-1.build.status"),
            old2_build = read_status(W, "old-2.build.status"),
            new_build = read_status(W, "new.build.status"),
            old1_test = read_status(W, "old-1.test.status"),
            new_test = read_status(W, "new.test.status"),
        },
    })
    local passed = true
    for _, r in ipairs(rows) do
        if r.verdict == "FAIL" then
            passed = false
        end
    end
    return { table = lib.render(rows), passed = passed, rows = rows }
end

local USAGE = "usage: parity.lua prepare P W | compare P W"

local COMMANDS = {
    compare = function(P, W)
        local result = M.compare(P, W)
        stdio.write(result.table .. "\n")
        if not result.passed then
            error("parity check failed", 0)
        end
    end,

    prepare = function(P, W)
        for _, copy in ipairs(M.prepare(P, W)) do
            stdio.write(copy .. "\n")
        end
    end,
}

-- `arg[0]` names the script that was started, also while this file is loaded by `require`.
if arg ~= nil and arg[0] ~= nil and path.basename(arg[0]) == "parity.lua" then
    local command, P, W = COMMANDS[arg[1]], arg[2], arg[3]
    if command == nil or P == nil or W == nil then
        stdio.error(USAGE .. "\n")
        error(USAGE, 0)
    end
    command(P, W)
end

return M
