-- Tests of parity_lib and parity. They run from the staged folder `cargo make skills` builds
-- (this folder's files plus the scripts under skills/*/scripts/), as `airsl test --allow-read . .`.
local parity = require("parity_lib")
local entry = require("parity")

local RECIPES = "fixtures/recipes/"
local MATCH = "fixtures/compare/match/"
local MISMATCH = "fixtures/compare/mismatch/"

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

-- The whole text `buildl doctor` printed for examples/python.
local DOCTOR = [[
workspace  /work/python
approver   manifest
execution  workers 0 (one per core), timeout 10m, strategy sandbox
remote: none
ceiling
  proc.run  python3
  env.read  -
  fs.read   .
  fs.write  .buildl, out
wants            ceiling  approved
  run python3    allowed  yes
  write .buildl  allowed  yes
  write out      allowed  yes
]]

-- An input for `rows` in which the old build and the buildl build agree on everything: the
-- spec example's mapping, a graph holding its two targets, one output with equal hashes.
local function agreeing()
    return {
        config = example(),
        recipes = { "all", "app", "clean", "deploy", "test" },
        graph = {
            targets = {
                ["//:app"] = { role = "Build", network = "Sealed" },
                ["//:unit"] = { role = "Test", network = "Sealed" },
            },
        },
        ceiling = { ["proc.run"] = { "go" }, ["env.read"] = { "HOME" }, ["fs.read"] = { "." } },
        hashes = { { old1 = "aa", old2 = "aa", new = "aa" } },
        status = { old1_build = 0, old2_build = 0, new_build = 0, old1_test = 0, new_test = 0 },
    }
end

-- Returns the rows of `check` (a string such as "3" or "4b") out of `list`.
local function of_check(list, check)
    local found = {}
    for _, row in ipairs(list) do
        if row.check == check then
            found[#found + 1] = row
        end
    end
    return found
end

-- Fails unless a row of `check` has `verdict` and, when `needle` is given, a subject and detail
-- that together hold it. Returns that row.
local function assert_row(list, check, verdict, needle)
    for _, row in ipairs(of_check(list, check)) do
        if row.verdict == verdict and (needle == nil or (row.subject .. " " .. row.detail):find(needle, 1, true)) then
            return row
        end
    end
    local seen = {}
    for _, row in ipairs(list) do
        seen[#seen + 1] = row.check .. " " .. row.verdict .. " " .. row.subject
    end
    error("no check " .. check .. " row " .. verdict .. " holding '" .. tostring(needle) .. "'; rows: " .. table.concat(seen, "; "), 0)
end

local function without_tests(input)
    input.config.old.test = nil
    return input
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

    doctor_ceiling_is_read_from_full_output = function()
        local got = parity.ceiling(DOCTOR)
        assert_names(got["proc.run"], { "python3" })
        assert_names(got["env.read"], {})
        assert_names(got["fs.read"], { "." })
        assert_names(got["fs.write"], { ".buildl", "out" })
    end,

    doctor_without_a_ceiling_is_an_error = function()
        local ok, err = pcall(parity.ceiling, "workspace  /w\napprover   manifest\n")
        assert(not ok, "expected an error")
        assert(tostring(err):find("ceiling", 1, true), tostring(err))
    end,

    an_unreadable_ceiling_line_is_named = function()
        local ok, err = pcall(parity.ceiling, "ceiling\n  proc.run  go\n  !!oops\nwants\n")
        assert(not ok, "expected an error")
        assert(tostring(err):find("!!oops", 1, true), tostring(err))
    end,

    everything_agreeing_passes = function()
        local got = parity.rows(agreeing())
        assert(#got > 0, "no rows")
        for _, row in ipairs(got) do
            assert(row.verdict == "PASS", row.check .. " " .. row.subject .. " " .. row.verdict)
        end
    end,

    an_unmapped_recipe_fails = function()
        local input = agreeing()
        input.recipes[#input.recipes + 1] = "lint"
        assert_row(parity.rows(input), "1", "FAIL", "lint")
    end,

    a_mapping_to_a_missing_target_fails = function()
        local input = agreeing()
        input.config.recipes.all = { target = "//:nowhere" }
        assert_row(parity.rows(input), "1", "FAIL", "//:nowhere")
    end,

    a_mapping_to_a_test_target_passes = function()
        local got = parity.rows(agreeing())
        local row = assert_row(got, "1", "PASS", "test")
        assert(row.detail:find("//:unit", 1, true), row.detail)
    end,

    a_failed_old_build_fails = function()
        local input = agreeing()
        input.status.old1_build = 2
        assert_row(parity.rows(input), "2", "FAIL", "old-1.build.status")
        input = agreeing()
        input.status.old1_build = nil
        assert_row(parity.rows(input), "2", "FAIL", "old-1.build.status")
    end,

    a_failed_buildl_build_fails = function()
        local input = agreeing()
        input.status.new_build = 1
        assert_row(parity.rows(input), "2", "FAIL", "new.build.status")
    end,

    a_changed_output_fails = function()
        local input = agreeing()
        input.hashes[1].new = "bb"
        assert_row(parity.rows(input), "3", "FAIL", "app")
    end,

    a_missing_output_fails = function()
        local input = agreeing()
        input.hashes[1].new = nil
        assert_row(parity.rows(input), "3", "FAIL", "app")
    end,

    an_unrepeatable_output_is_not_a_failure = function()
        local input = agreeing()
        input.hashes[1].old2 = "cc"
        input.hashes[1].new = "bb"
        local got = parity.rows(input)
        assert_row(got, "4", "NOT REPEATABLE", "app")
        assert_row(got, "5", "PASS")
        for _, row in ipairs(got) do
            assert(row.verdict ~= "FAIL", row.check .. " " .. row.subject)
        end
    end,

    an_unrepeatable_output_without_tests_fails = function()
        local input = without_tests(agreeing())
        input.hashes[1].old2 = "cc"
        assert_row(parity.rows(input), "4", "FAIL", "app")
    end,

    a_judged_output_defers_to_tests = function()
        local input = agreeing()
        input.config.outputs[1].judge = "tests"
        input.config.outputs[1].reason = "object code differs by path"
        input.hashes[1].new = "bb"
        local got = parity.rows(input)
        assert_row(got, "4b", "JUDGED BY TESTS", "object code differs by path")
        assert(#of_check(got, "3") == 0, "a judged output has no check 3 row")
    end,

    a_judged_output_without_tests_fails = function()
        local input = without_tests(agreeing())
        input.config.outputs[1].judge = "tests"
        input.config.outputs[1].reason = "object code differs by path"
        assert_row(parity.rows(input), "4b", "FAIL", "app")
    end,

    differing_test_outcomes_fail = function()
        local input = agreeing()
        input.status.new_test = 1
        assert_row(parity.rows(input), "5", "FAIL")
    end,

    no_old_test_reads_no_tests = function()
        local got = parity.rows(without_tests(agreeing()))
        assert_row(got, "5", "NO TESTS")
        for _, row in ipairs(got) do
            assert(row.verdict ~= "FAIL", row.check .. " " .. row.subject)
        end
    end,

    a_wider_program_ceiling_fails = function()
        local input = agreeing()
        input.ceiling["proc.run"] = { "go", "curl" }
        assert_row(parity.rows(input), "6", "FAIL", "curl")
    end,

    a_wider_env_ceiling_fails = function()
        local input = agreeing()
        input.ceiling["env.read"] = { "HOME", "TOKEN" }
        assert_row(parity.rows(input), "6", "FAIL", "TOKEN")
    end,

    an_unexpected_network_target_fails = function()
        local input = agreeing()
        input.graph.targets["//:app"].network = "Declared"
        assert_row(parity.rows(input), "6", "FAIL", "//:app")
    end,

    a_relative_project_is_refused = function()
        local ok, err = pcall(parity.check_paths, "p", "/w", nil)
        assert(not ok, "expected an error")
        assert(tostring(err):find("project folder must be absolute: p", 1, true), tostring(err))
    end,

    a_relative_work_folder_is_refused = function()
        local ok, err = pcall(parity.check_paths, "/p", "w", nil)
        assert(not ok, "expected an error")
        assert(tostring(err):find("work folder must be absolute: w", 1, true), tostring(err))
    end,

    a_work_folder_inside_the_project_is_refused = function()
        local ok, err = pcall(parity.check_paths, "/p", "/p/w", nil)
        assert(not ok, "expected an error")
        assert(tostring(err):find("outside the project", 1, true), tostring(err))
        assert(not pcall(parity.check_paths, "/p", "/p", nil), "the project itself is no work folder")
        assert(not pcall(parity.check_paths, "/p/", "/p/../p/w", nil), "a path is judged normalized")
        assert(pcall(parity.check_paths, "/p", "/pw", nil), "a sibling whose name starts with p is outside")
    end,

    a_non_empty_work_folder_is_refused = function()
        local ok, err = pcall(parity.check_paths, "/p", "/w", { "x" })
        assert(not ok, "expected an error")
        assert(tostring(err):find("work folder /w is not empty", 1, true), tostring(err))
    end,

    an_absent_or_empty_work_folder_is_accepted = function()
        assert(pcall(parity.check_paths, "/p", "/w", nil))
        assert(pcall(parity.check_paths, "/p", "/w", {}))
    end,

    outputs_and_state_are_left_out = function()
        local got = parity.left_out(example())
        for _, path in ipairs({ ".git", ".buildl", "out", "parity.json", "app", "out/app" }) do
            assert(got[path], "not left out: " .. path)
        end
        local decoded = example()
        decoded.skip = { "build", "./dist" }
        got = parity.left_out(decoded)
        assert(got["build"] and got["dist"], "skip entries are left out, written plainly")
        assert(not got["src"], "other paths are copied")
    end,

    an_output_path_outside_the_copy_is_refused = function()
        for _, bad in ipairs({ "../new/out/out.txt", "/etc/passwd", "a/../../x", ".." }) do
            local decoded = example()
            decoded.outputs[1].old = bad
            assert_refused(decoded, "outputs[1].old")
            decoded = example()
            decoded.outputs[1].new = bad
            assert_refused(decoded, "outputs[1].new")
        end
        local decoded = example()
        decoded.outputs[1].old = "./dist/../app"
        assert(parity.config(decoded) == decoded, "a path that stays inside is accepted")
    end,

    an_empty_outputs_list_is_refused = function()
        local decoded = example()
        decoded.outputs = {}
        assert_refused(decoded, "outputs")
    end,

    a_recipe_with_target_and_dropped_is_refused = function()
        local decoded = example()
        decoded.recipes.all = { target = "//:app", dropped = "also dropped" }
        assert_refused(decoded, "recipes.all")
    end,

    a_judge_other_than_tests_is_refused = function()
        local decoded = example()
        decoded.outputs[1].judge = "eyes"
        decoded.outputs[1].reason = "looks the same"
        assert_refused(decoded, "outputs[1].judge")
    end,

    a_ceiling_without_proc_run_is_an_error = function()
        local input = agreeing()
        input.ceiling["proc.run"] = nil
        local ok, err = pcall(parity.rows, input)
        assert(not ok, "expected an error")
        assert(tostring(err):find("proc.run", 1, true), tostring(err))
    end,

    a_ceiling_without_env_read_is_an_error = function()
        local input = agreeing()
        input.ceiling["env.read"] = nil
        local ok, err = pcall(parity.rows, input)
        assert(not ok, "expected an error")
        assert(tostring(err):find("env.read", 1, true), tostring(err))
    end,

    a_stale_mapping_is_still_checked = function()
        local input = agreeing()
        input.config.recipes.gone = { target = "//:gone" }
        assert_row(parity.rows(input), "1", "FAIL", "gone")
        input = agreeing()
        input.config.recipes.gone = { target = "//:app" }
        assert_row(parity.rows(input), "1", "PASS", "gone")
    end,

    a_missing_test_status_fails = function()
        local input = agreeing()
        input.status.new_test = nil
        assert_row(parity.rows(input), "5", "FAIL", "new.test.status is missing")
        input = agreeing()
        input.status.old1_test = nil
        assert_row(parity.rows(input), "5", "FAIL", "old-1.test.status is missing")
    end,

    a_status_file_without_a_number_is_named = function()
        local input = agreeing()
        input.status.new_build = false
        assert_row(parity.rows(input), "2", "FAIL", "new.build.status holds no exit code")
        input = agreeing()
        input.status.old1_test = false
        assert_row(parity.rows(input), "5", "FAIL", "old-1.test.status holds no exit code")
    end,

    check_5_names_both_codes = function()
        local input = agreeing()
        input.status.old1_test = 2
        input.status.new_test = 1
        local row = assert_row(parity.rows(input), "5", "PASS")
        assert(row.detail:find("2", 1, true) and row.detail:find("1", 1, true), row.detail)
    end,

    outputs_left_to_tests_need_passing_tests = function()
        local input = agreeing()
        input.hashes[1].old2 = "cc"
        input.status.old1_test = 1
        input.status.new_test = 1
        assert_row(parity.rows(input), "5", "FAIL", "left to the tests")

        input = agreeing()
        input.config.outputs[1].judge = "tests"
        input.config.outputs[1].reason = "object code differs by path"
        input.status.new_test = 1
        assert_row(parity.rows(input), "5", "FAIL", "left to the tests")

        input = agreeing()
        input.config.outputs[1].judge = "tests"
        input.config.outputs[1].reason = "object code differs by path"
        input.status.old1_test = 1
        assert_row(parity.rows(input), "5", "FAIL", "left to the tests")
    end,

    a_work_folder_inside_the_real_project_is_refused = function()
        local ok, err = pcall(parity.check_outside, "/private/tmp/x", "/private/tmp/x/w")
        assert(not ok, "expected an error")
        assert(tostring(err):find("outside the project", 1, true), tostring(err))
        assert(pcall(parity.check_outside, "/private/tmp/x", "/private/tmp/xw"))
    end,

    render_ends_with_the_verdict = function()
        local input = agreeing()
        input.hashes[1].new = "bb"
        local text = parity.render(parity.rows(input))
        assert(text:match("[^\n]+$") == "parity: FAIL (1 rows)", text)
        assert(text:find("FAIL", 1, true) and text:find("app", 1, true), text)
    end,

    render_reads_pass_when_nothing_fails = function()
        local text = parity.render(parity.rows(agreeing()))
        assert(text:match("[^\n]+$") == "parity: PASS", text)
        local lines = 0
        for _ in text:gmatch("[^\n]+") do
            lines = lines + 1
        end
        assert(lines == #parity.rows(agreeing()) + 1, "one line per row, then the verdict")
    end,

    compare_passes_on_the_match_fixture = function()
        local got = entry.compare(MATCH .. "P", MATCH .. "W")
        assert(got.passed, got.table)
        assert(got.table:match("[^\n]+$") == "parity: PASS", got.table)
    end,

    compare_fails_on_the_mismatch_fixture = function()
        local got = entry.compare(MISMATCH .. "P", MISMATCH .. "W")
        assert(not got.passed, got.table)
        local found = false
        for line in got.table:gmatch("[^\n]+") do
            if line:find("FAIL", 1, true) and line:find("out.txt", 1, true) and line:match("^%s*3%s") then
                found = true
            end
        end
        assert(found, "no check 3 FAIL row naming out.txt:\n" .. got.table)
    end,

    a_path_under_a_left_out_folder_is_left_out = function()
        local left = parity.left_out(example())
        assert(parity.is_left_out("out", left))
        assert(parity.is_left_out("out/app", left))
        assert(parity.is_left_out(".git/objects/ab/cd", left))
        assert(parity.is_left_out("app", left))
        assert(not parity.is_left_out("outside/app", left), "a longer name is not below out")
        assert(not parity.is_left_out("src/app", left), "only the project's own app path is left out")
    end,
}
