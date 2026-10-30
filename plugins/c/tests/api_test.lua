-- Tests of the plugin's public table. Run from a folder holding the modules: airsl test --allow-read . .
local api = require("api")
local fake_buildl = require("fake_buildl")

-- Returns the public table over a fresh fake whose `folder()` is `folder` ("cpp" unless given).
local function plugin(results, folder)
    local fake = fake_buildl.new(folder or "cpp", results)
    return api.new(fake), fake
end

local function same(got, want, what)
    what = what or "list"
    assert(type(got) == "table", what .. " is " .. tostring(got) .. ", want a list")
    assert(#got == #want, what .. " has " .. #got .. " items, want " .. #want .. ": " .. table.concat(got, " "))
    for i = 1, #want do
        assert(got[i] == want[i], what .. " item " .. i .. " is " .. tostring(got[i]) .. ", want " .. want[i])
    end
end

local function refuses(run, words)
    local ok, err = pcall(run)
    assert(not ok, "expected an error")
    err = tostring(err)
    for _, word in ipairs(words) do
        assert(err:find(word, 1, true), word .. " is not in " .. err)
    end
    return err
end

local function count(fake, name)
    local found = 0
    for _, declaration in ipairs(fake.declared) do
        if declaration.name == name then
            found = found + 1
        end
    end
    return found
end

return {
    the_public_table_holds_every_function = function()
        local c = plugin()
        local want = {
            "binary",
            "library",
            "shared_library",
            "test",
            "platform",
            "includes",
            "defines",
            "shared_name",
            "soname",
            "rpath",
            "depfile",
        }
        local seen = {}
        for _, key in ipairs(want) do
            assert(c[key] ~= nil, "the table has no " .. key)
            seen[key] = true
        end
        for key in pairs(c) do
            assert(seen[key], "the table holds " .. tostring(key) .. ", which is not public")
        end
        assert(type(c.depfile.parse) == "function", "depfile.parse is not a function")
    end,

    binary_declares_the_discovery_of_each_source = function()
        local c, fake = plugin()
        c.binary("app", { srcs = { "src/a.c" } })
        assert(fake.find("app.src.a.c.deps").kind == "discover")
        assert(fake.find("app") == nil, "the link is declared before discovery")
    end,

    each_function_declares_its_last_step = function()
        local found = {
            [":t.src.a.c.deps"] = "obj: cpp/src/a.c\n",
        }
        local c, fake = plugin(found)
        c.binary("t", { srcs = { "src/a.c" } })
        assert(fake.find("t").kind == "target")

        c, fake = plugin(found)
        c.library("t", { srcs = { "src/a.c" } })
        assert(fake.find("t").options.run[1] == "sh")

        c, fake = plugin(found)
        c.shared_library("t", { srcs = { "src/a.c" } })
        same(fake.find("t").options.run, { "cc", "-shared", "-o", "$out", "$deps" }, "run")

        c, fake = plugin(found)
        c.test("t", { srcs = { "src/a.c" } })
        assert(fake.find("t.bin").kind == "target")
        assert(fake.find("t").kind == "test")
    end,

    a_function_returns_nothing = function()
        local c = plugin()
        assert(c.binary("app", { srcs = { "src/a.c" } }) == nil)
    end,

    includes_use_the_build_files_folder = function()
        local c = plugin(nil, "cpp")
        same(c.includes({ "include", "third/party" }), { "-Icpp/include", "-Icpp/third/party" }, "includes")
        c = plugin(nil, "")
        same(c.includes({ "include" }), { "-Iinclude" }, "includes at the root")
    end,

    includes_refuses_what_is_not_a_list_of_strings = function()
        local c = plugin()
        local err = refuses(function()
            c.includes("include")
        end, { "c.includes", "list" })
        assert(err:find("api_test", 1, true), "the location is not this file's: " .. err)
        assert(not err:find("api.lua", 1, true), "the location is the plugin's: " .. err)
        err = refuses(function()
            c.includes({ "include", 7 })
        end, { "c.includes", "string" })
        assert(err:find("api_test", 1, true), "the location is not this file's: " .. err)
        assert(not err:find("api.lua", 1, true), "the location is the plugin's: " .. err)
    end,

    the_flag_helpers_are_the_flags_modules = function()
        local c = plugin()
        same(c.defines({ B = true, A = "1", C = false }), { "-DA=1", "-DB" }, "defines")
        assert(c.shared_name("util", "linux") == "libutil.so")
        assert(c.shared_name("util", "macos") == "libutil.dylib")
        assert(c.shared_name("util", nil) == nil)
        same(c.soname("libutil.so", "linux"), { "-Wl,-soname,libutil.so" }, "soname")
        same(c.rpath("macos"), { "-Wl,-rpath,@loader_path" }, "rpath")
        same(c.rpath(nil), {}, "rpath before discovery")
    end,

    depfile_parse_is_exposed = function()
        local c = plugin()
        local rules = c.depfile.parse("obj: a.c b.h\n")
        assert(#rules == 1 and rules[1].target == "obj")
        same(rules[1].prerequisites, { "a.c", "b.h" }, "prerequisites")
    end,

    platform_is_nil_before_discovery = function()
        local c, fake = plugin()
        assert(c.platform() == nil)
        assert(count(fake, "c.platform") == 1, "c.platform is not declared once")
        local found = fake.find("c.platform")
        assert(found.kind == "discover")
        same(found.options.run, { "cc", "-dumpmachine" }, "run")
        assert(found.options.stdout == true, "stdout is not set")
    end,

    platform_is_declared_once_per_instance = function()
        local c, fake = plugin({ [":c.platform"] = "x86_64-apple-darwin23.0.0\n" })
        local first = c.platform()
        local second = c.platform({})
        assert(first == "macos" and second == "macos", tostring(first) .. " " .. tostring(second))
        assert(count(fake, "c.platform") == 1, "c.platform is declared more than once")

        local d, other = plugin()
        assert(d.platform() == nil)
        assert(count(other, "c.platform") == 1, "an instance shares the declaration of another")
    end,

    platform_with_the_same_cc_is_not_refused = function()
        local c, fake = plugin()
        c.platform({ cc = "clang" })
        c.platform({ cc = "clang" })
        same(fake.find("c.platform").options.run, { "clang", "-dumpmachine" }, "run")
    end,

    platform_with_another_cc_is_refused = function()
        local c = plugin()
        c.platform()
        local err = refuses(function()
            c.platform({ cc = "clang" })
        end, { "c.platform", "clang" })
        assert(err:find(":%d+:"), "no location in " .. err)
    end,

    platform_reads_the_triple = function()
        local c = plugin({ [":c.platform"] = "aarch64-linux-gnu\n" })
        assert(c.platform() == "linux")
    end,

    platform_refuses_a_triple_that_is_neither = function()
        local c = plugin({ [":c.platform"] = "riscv64-unknown-elf\n" })
        refuses(function()
            c.platform()
        end, { "c.platform", "riscv64-unknown-elf" })
    end,

    platform_refuses_a_bad_option = function()
        local c = plugin()
        refuses(function()
            c.platform({ cc = 1 })
        end, { "c.platform", "cc" })
        refuses(function()
            c.platform({ flavour = "x" })
        end, { "c.platform", "flavour" })
    end,

    a_bad_option_has_the_callers_location = function()
        local c = plugin()
        local ok, err = pcall(function()
            c.binary("app", { srcs = { "src/a.c" }, bogus = true })
        end)
        assert(not ok, "expected an error")
        err = tostring(err)
        assert(err:find("c.binary(app)", 1, true), err)
        assert(err:find("bogus", 1, true), err)
        assert(err:find("api_test", 1, true), "the location is not this file's: " .. err)
        assert(not err:find("api.lua", 1, true), "the location is the plugin's: " .. err)
    end,

    a_bad_platform_triple_has_the_callers_location = function()
        local c = plugin({ [":c.platform"] = "riscv64-unknown-elf\n" })
        local ok, err = pcall(function()
            c.platform()
        end)
        assert(not ok, "expected an error")
        err = tostring(err)
        assert(err:find("api_test", 1, true), "the location is not this file's: " .. err)
        assert(not err:find("api.lua", 1, true), "the location is the plugin's: " .. err)
    end,

    an_error_from_the_graph_has_the_callers_location = function()
        local c = plugin({ [":app.src.a.c.deps"] = "other: cpp/src/a.c\n" })
        local ok, err = pcall(function()
            c.binary("app", { srcs = { "src/a.c" } })
        end)
        assert(not ok, "expected an error")
        err = tostring(err)
        assert(err:find("c.binary(app)", 1, true), err)
        assert(err:find("src/a.c", 1, true), err)
        assert(err:find("api_test", 1, true), "the location is not this file's: " .. err)
        assert(not err:find("api.lua", 1, true), "the location is the plugin's: " .. err)
    end,
}
