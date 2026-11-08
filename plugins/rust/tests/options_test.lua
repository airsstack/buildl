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
    options_may_be_left_out = function()
        local got = options.check("binary", "hello", nil)
        same(got.args, {})
        same(got.deps, {})
        assert(got.profile == "dev", tostring(got.profile))
        assert(got.profile_dir == "debug", tostring(got.profile_dir))
        assert(got.output == "hello", tostring(got.output))
        assert(got.kind == nil, "kind is set")
        assert(got.version_probe == false)
        same(options.check("test", "unit", nil).args, {})
    end,

    options_must_be_a_table = function()
        refuses("binary", "x", "rust.binary(app): the options must be a table")
    end,

    an_unknown_option_is_refused = function()
        refuses("binary", { bogus = 1 }, "rust.binary(app): the option bogus is not known")
        refuses("binary", { zeta = 1, bogus = 1 }, "the option bogus is not known")
    end,

    a_wrong_type_is_refused = function()
        refuses("binary", { args = "x" }, "rust.binary(app): the option args must be a list of strings")
        refuses("binary", { args = { 1 } }, "the option args must be a list of strings")
        refuses("binary", { deps = "x" }, "the option deps must be a list of strings")
        refuses("binary", { profile = {} }, "the option profile must be a string")
        refuses("binary", { output = 1 }, "the option output must be a string")
        refuses("binary", { version_probe = "yes" }, "the option version_probe must be a boolean")
        refuses("library", { kind = 1 }, "the option kind must be a string")
        refuses("binary", { profile = 1, output = 2 }, "the option output ")
    end,

    kind_is_refused_outside_library = function()
        refuses("binary", { kind = "staticlib" }, "rust.binary(app): the option kind is refused")
        refuses("test", { kind = "staticlib" }, "rust.test(app): the option kind is refused")
    end,

    library_needs_a_kind = function()
        refuses("library", {}, 'rust.library(app): kind must be "staticlib" or "cdylib"')
        refuses("library", nil, 'kind must be "staticlib" or "cdylib"')
        refuses("library", { kind = "rlib" }, 'kind must be "staticlib" or "cdylib"')
        assert(options.check("library", "app", { kind = "staticlib" }).kind == "staticlib")
        assert(options.check("library", "app", { kind = "cdylib" }).kind == "cdylib")
    end,

    outputs_default_by_kind = function()
        assert(options.check("binary", "hello", {}).output == "hello")
        assert(options.check("library", "mylib", { kind = "staticlib" }).output == "libmylib.a")
        assert(options.check("library", "ffi", { kind = "cdylib" }).output == "ffi")
        assert(options.check("library", "ffi", { kind = "cdylib", output = "libffi.so" }).output == "libffi.so")
        assert(options.check("binary", "hello", { output = "x" }).output == "x")
        assert(options.check("test", "unit", {}).output == nil)
    end,

    profile_folders = function()
        local function dir(profile)
            return options.check("binary", "app", { profile = profile }).profile_dir
        end
        assert(dir("dev") == "debug")
        assert(dir("test") == "debug")
        assert(dir("release") == "release")
        assert(dir("bench") == "release")
        assert(dir("fast") == "fast")
        assert(options.check("binary", "app", { profile = "fast" }).profile == "fast")
    end,
}
