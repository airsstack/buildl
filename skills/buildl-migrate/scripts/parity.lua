-- The parity check's entry. Run as a script: `parity.lua prepare P W`. `P` is the project
-- folder and `W` a work folder; both are absolute paths. All judging is in `parity_lib`.
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
function M.prepare(P, W)
    lib.check_paths(P, W, nil)
    local config = read_config(P)
    lib.check_paths(P, W, fs.exists(W) and fs.list(W) or nil)

    local left = lib.left_out(config)
    local copies = { path.join(W, "old-1"), path.join(W, "old-2"), path.join(W, "new") }
    for _, copy in ipairs(copies) do
        fs.mkdir(copy)
    end
    for _, rel in ipairs(fs.walk(P)) do
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

local USAGE = "usage: parity.lua prepare P W | compare P W"

local COMMANDS = {
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
