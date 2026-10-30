-- A stand-in for the `buildl` table that records what a plugin declares and serves canned
-- discovery output. The shapes follow the build file API: `target`, `test` and `discover` take
-- `(name, options)`, `discovered` takes a label, `folder` none.
local M = {}

-- The options each declaration takes.
local ALLOWED = {
    target = { "run", "rule", "inputs", "deps", "outputs", "env", "network", "stdout", "always" },
    test = { "run", "rule", "inputs", "deps", "outputs", "env", "network", "always" },
    discover = { "run", "rule", "inputs", "deps", "env", "network", "stdout" },
}

-- Returns whether `list` holds `item`.
local function has(list, item)
    for _, entry in ipairs(list) do
        if entry == item then
            return true
        end
    end
    return false
end

-- Returns a fake whose `folder()` is `folder` and whose `discovered(label)` is `results[label]`
-- (nil when the label is not a key). `declared` lists the declarations in the order made, each
-- `{ kind = "target"|"test"|"discover", name, options }`; `find(name)` returns the one named
-- `name` or nil. A name outside letters, digits, `.`, `-` and `_`, a name declared twice and an
-- option the declaration does not take raise, as the real `buildl` does.
function M.new(folder, results)
    local fake = { declared = {} }

    local function record(kind)
        return function(name, options)
            assert(type(name) == "string" and name:match("^[%w._-]+$"), "invalid target name: " .. tostring(name))
            assert(fake.find(name) == nil, "target declared twice: " .. name)
            for key in pairs(options) do
                assert(has(ALLOWED[kind], key), kind .. " takes no option " .. tostring(key))
            end
            assert((options.run == nil) ~= (options.rule == nil), "exactly one of run and rule")
            fake.declared[#fake.declared + 1] = { kind = kind, name = name, options = options }
        end
    end

    fake.target = record("target")
    fake.test = record("test")
    fake.discover = record("discover")

    function fake.discovered(label)
        return (results or {})[label]
    end

    function fake.folder()
        return folder
    end

    function fake.find(name)
        for _, declaration in ipairs(fake.declared) do
            if declaration.name == name then
                return declaration
            end
        end
        return nil
    end

    return fake
end

return M
