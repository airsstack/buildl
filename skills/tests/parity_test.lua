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
}
