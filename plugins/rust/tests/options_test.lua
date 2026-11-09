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
        assert(got.libraries == nil, "libraries is set")
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
        refuses("library", { kind = 1 }, "the option kind must be a list of strings")
        refuses("library", { kind = "staticlib" }, "the option kind must be a list of strings")
        refuses("binary", { profile = 1, output = 2 }, "the option output ")
    end,

    kind_is_refused_outside_library = function()
        refuses("binary", { kind = { "staticlib" } }, "rust.binary(app): the option kind is refused")
        refuses("test", { kind = { "staticlib" } }, "rust.test(app): the option kind is refused")
    end,

    output_is_refused_outside_binary = function()
        refuses("test", { output = "x" }, "rust.test(app): the option output is refused")
        refuses("library", { kind = { "staticlib" }, output = "x" }, "rust.library(app): the option output is refused")
        assert(options.check("test", "app", nil).output == nil)
    end,

    library_needs_a_kind = function()
        local word = 'kind must list "staticlib", "cdylib" or both'
        refuses("library", {}, "rust.library(app): " .. word)
        refuses("library", nil, word)
        refuses("library", { kind = {} }, word)
        refuses("library", { kind = { "rlib" } }, word)
        refuses("library", { kind = { "cdylib", "rlib" } }, word)
        refuses("library", { kind = { "cdylib", "staticlib", "cdylib" } }, "rust.library(app): kind lists cdylib twice")
    end,

    library_files_are_named_as_cargo_names_them = function()
        local function files(kinds, platform)
            local got = {}
            for i, library in ipairs(options.check("library", "ffi", { kind = kinds }, platform).libraries) do
                got[i] = library.kind .. "=" .. library.output
            end
            return got
        end
        same(files({ "staticlib" }), { "staticlib=libffi.a" })
        same(files({ "cdylib" }, "linux"), { "cdylib=libffi.so" })
        same(files({ "cdylib" }, "macos"), { "cdylib=libffi.dylib" })
        same(files({ "cdylib" }, nil), { "cdylib=ffi" })
        same(files({ "cdylib", "staticlib" }, "macos"), { "cdylib=libffi.dylib", "staticlib=libffi.a" })
        same(files({ "staticlib", "cdylib" }, "linux"), { "staticlib=libffi.a", "cdylib=libffi.so" })
        assert(options.check("library", "ffi", { kind = { "staticlib" } }).output == nil, "a library has output")
    end,

    outputs_default_by_kind = function()
        assert(options.check("binary", "hello", {}).output == "hello")
        assert(options.check("binary", "hello", { output = "x" }).output == "x")
        assert(options.check("test", "unit", {}).output == nil)
        assert(options.check("binary", "hello", {}).libraries == nil, "a binary has libraries")
    end,

    shared_name_follows_the_platform = function()
        assert(options.shared_name("ffi", "linux") == "libffi.so")
        assert(options.shared_name("ffi", "macos") == "libffi.dylib")
        assert(options.shared_name("ffi", nil) == nil)
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
