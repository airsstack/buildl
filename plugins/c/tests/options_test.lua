-- Tests of options. Run from a folder holding the module: airsl test --allow-read . .
local options = require("options")

local function refuses(kind, given, word)
    local ok, err = pcall(options.check, kind, "app", given)
    assert(not ok, "expected an error")
    assert(tostring(err):find(word, 1, true), tostring(err))
end

local function same(got, want)
    assert(#got == #want, "length " .. #got .. ", want " .. #want)
    for i = 1, #want do
        assert(got[i] == want[i], "item " .. i .. " is " .. tostring(got[i]) .. ", want " .. want[i])
    end
end

return {
    an_unknown_option_is_refused = function()
        refuses("binary", { srcs = { "a.c" }, cflag = {} }, "cflag")
    end,

    an_unknown_kind_is_refused = function()
        refuses("archive", { srcs = { "a.c" } }, "c.archive(app)")
    end,

    options_that_are_not_a_table_are_refused = function()
        refuses("binary", "a.c", "must be a table")
    end,

    the_first_wrong_type_by_name_is_named = function()
        refuses("binary", { srcs = { "a.c" }, output = 1, cc = 2 }, "option cc ")
    end,

    an_error_names_the_function_and_the_target_and_has_a_location = function()
        local ok, err = pcall(function()
            return (options.check("shared_library", "util", {}))
        end)
        assert(not ok, "expected an error")
        err = tostring(err)
        assert(err:find("c.shared_library(util)", 1, true), err)
        assert(err:find(":%d+:"), "no location in " .. err)
    end,

    srcs_is_required_and_not_empty = function()
        refuses("binary", {}, "srcs")
        refuses("binary", { srcs = {} }, "srcs")
    end,

    a_wrong_type_is_refused = function()
        refuses("binary", { srcs = { "a.c" }, cflags = "-O2" }, "cflags")
        refuses("binary", { srcs = { "a.c" }, cflags = { 1 } }, "cflags")
        refuses("binary", { srcs = { "a.c" }, cc = {} }, "cc")
        refuses("binary", { srcs = { "a.c" }, version_probe = "yes" }, "version_probe")
        refuses("binary", { srcs = "a.c" }, "srcs")
    end,

    an_unknown_extension_is_refused = function()
        refuses("binary", { srcs = { "a.c", "b.f90" } }, "b.f90")
        refuses("binary", { srcs = { "Makefile" } }, "Makefile")
    end,

    library_refuses_link_options = function()
        refuses("library", { srcs = { "a.c" }, ldflags = {} }, "ldflags")
        refuses("library", { srcs = { "a.c" }, ldlibs = {} }, "ldlibs")
        refuses("library", { srcs = { "a.c" }, linker = "cc" }, "linker")
        refuses("library", { srcs = { "a.c" }, deps = {} }, "deps")
        for _, kind in ipairs({ "binary", "shared_library", "test" }) do
            options.check(kind, "app", { srcs = { "a.c" }, ldflags = {}, ldlibs = {}, linker = "cc", deps = {} })
        end
    end,

    defaults_are_filled = function()
        local got = options.check("binary", "app", { srcs = { "a.c" } })
        assert(got.cc == "cc")
        assert(got.cxx == "c++")
        assert(got.version_probe == false)
        assert(got.output == "app")
        for _, key in ipairs({ "headers", "header_deps", "cflags", "cxxflags", "ldflags", "ldlibs", "deps" }) do
            same(got[key], {})
        end
        assert(options.check("test", "app", { srcs = { "a.c" } }).output == "app.bin")
        assert(options.check("shared_library", "app", { srcs = { "a.c" } }).output == "app")
        assert(options.check("binary", "app", { srcs = { "a.c" }, output = "x" }).output == "x")
    end,

    sources_are_tagged_by_language = function()
        local got = options.check("binary", "app", { srcs = { "a.c", "b.cc", "c.cpp", "d.cxx" } })
        local want = { "c", "cpp", "cpp", "cpp" }
        local paths = { "a.c", "b.cc", "c.cpp", "d.cxx" }
        for i = 1, 4 do
            assert(got.srcs[i].path == paths[i], got.srcs[i].path)
            assert(got.srcs[i].language == want[i], got.srcs[i].language)
        end
    end,

    linker_follows_the_language = function()
        assert(options.check("binary", "app", { srcs = { "a.c" } }).linker == "cc")
        assert(options.check("binary", "app", { srcs = { "a.c", "b.cpp" } }).linker == "c++")
        assert(options.check("binary", "app", { srcs = { "a.c" }, cc = "gcc" }).linker == "gcc")
        assert(options.check("binary", "app", { srcs = { "a.cc" }, cxx = "g++" }).linker == "g++")
        assert(options.check("binary", "app", { srcs = { "a.cpp" }, linker = "ld" }).linker == "ld")
    end,
}
