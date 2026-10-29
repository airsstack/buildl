-- Tests of parity_lib. Run from this folder: airsl test --allow-read . .
local parity = require("parity_lib")

local RECIPES = "fixtures/recipes/"

local function read(rel)
    return airsstack.fs.read(RECIPES .. rel)
end

-- Fails unless `got` holds exactly the names of `want`, in order.
local function assert_names(got, want)
    assert(#got == #want, "got {" .. table.concat(got, ", ") .. "}, want {" .. table.concat(want, ", ") .. "}")
    for i, name in ipairs(want) do
        assert(got[i] == name, "item " .. i .. ": got " .. tostring(got[i]) .. ", want " .. name)
    end
end

-- The example parity.json of the spec, decoded fresh on each call so a test may change it.
local function example()
    return airsstack.json.decode([[
{
  "tool": "make",
  "old": {
    "build":    ["make", "all"],
    "test":     ["make", "test"],
    "programs": ["go"],
    "env":      ["HOME"],
    "network":  false
  },
  "recipes": {
    "all":    { "target": "//:app" },
    "app":    { "target": "//:app" },
    "test":   { "target": "//:unit" },
    "clean":  { "dropped": "buildl clean replaces it" },
    "deploy": { "dropped": "kept in Makefile: not a build step (user's answer)" }
  },
  "outputs": [ { "old": "app", "new": "out/app" } ],
  "skip": []
}
]])
end

-- Fails unless `parity.config(decoded)` raises with a message holding `needle`.
local function assert_refused(decoded, needle)
    local ok, err = pcall(parity.config, decoded)
    assert(not ok, "expected an error naming " .. needle)
    assert(tostring(err):find(needle, 1, true), "error lacks '" .. needle .. "': " .. tostring(err))
end

return {
    make_lists_rules_not_assignments = function()
        local got = parity.recipes("make", { Makefile = read("make/Makefile") })
        assert_names(got, { "%.o", "all", "app", "test" })
    end,

    just_lists_recipes_not_settings = function()
        local got = parity.recipes("just", { justfile = read("just/justfile") })
        assert_names(got, { "build", "test" })
    end,

    npm_lists_script_keys = function()
        local got = parity.recipes("npm", { ["package.json"] = read("npm/package.json") })
        assert_names(got, { "build", "lint", "test" })
    end,

    shell_lists_every_script = function()
        local got = parity.recipes("shell", {
            ["build.sh"] = read("shell/build.sh"),
            ["ci/test.sh"] = read("shell/ci/test.sh"),
            ["README.md"] = read("shell/README.md"),
        })
        assert_names(got, { "build.sh", "ci/test.sh" })
    end,

    an_unknown_tool_is_an_error = function()
        local ok, err = pcall(parity.recipes, "cmake", {})
        assert(not ok, "expected an error")
        assert(tostring(err):find("cmake", 1, true), tostring(err))
    end,

    a_complete_config_is_accepted = function()
        local decoded = example()
        local got = parity.config(decoded)
        assert(got == decoded, "config returns the table it was given")
        assert(got.tool == "make")
        assert(got.recipes.deploy.dropped ~= nil)
    end,

    a_missing_field_is_named = function()
        local decoded = example()
        decoded.old.network = nil
        assert_refused(decoded, "old.network")
    end,

    an_unknown_tool_is_refused = function()
        local decoded = example()
        decoded.tool = "cmake"
        assert_refused(decoded, "tool")
    end,

    a_drop_without_reason_is_refused = function()
        local decoded = example()
        decoded.recipes.clean = { dropped = "" }
        assert_refused(decoded, "clean")
    end,

    judge_without_reason_is_refused = function()
        local decoded = example()
        decoded.outputs[1].judge = "tests"
        assert_refused(decoded, "outputs[1]")
    end,

    old_test_may_be_absent = function()
        local decoded = example()
        decoded.old.test = nil
        decoded.skip = nil
        assert(parity.config(decoded) == decoded)
    end,
}
