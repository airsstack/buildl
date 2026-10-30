-- Tests of flags. Run from a folder holding the module: airsl test --allow-read . .
local flags = require("flags")

local function same(got, want)
    assert(#got == #want, "length " .. #got .. ", want " .. #want)
    for i = 1, #want do
        assert(got[i] == want[i], "item " .. i .. " is " .. tostring(got[i]) .. ", want " .. want[i])
    end
end

-- Calls `fn` through a Lua function, so the error carries a location, and checks that the
-- message holds `name`, the public function, and `value`.
local function refuses(fn, name, value)
    local ok, err = pcall(function()
        return (fn(value))
    end)
    assert(not ok, "expected an error")
    err = tostring(err)
    assert(err:find(name, 1, true), "no " .. name .. " in " .. err)
    assert(err:find(value, 1, true), "no " .. value .. " in " .. err)
    assert(err:find(":%d+:"), "no location in " .. err)
end

return {
    includes_at_the_root = function()
        same(flags.includes("", { "include" }), { "-Iinclude" })
    end,

    includes_below_the_root = function()
        same(flags.includes("cpp", { "include" }), { "-Icpp/include" })
    end,

    defines_are_sorted_and_typed = function()
        same(flags.defines({ B = "1", A = true, C = false, D = 2 }), { "-DA", "-DB=1", "-DD=2" })
    end,

    defines_refuses_a_value_that_is_not_a_boolean_string_or_number = function()
        refuses(function(key)
            return (flags.defines({ [key] = {} }))
        end, "c.defines", "LEVEL")
    end,

    defines_refuses_a_key_that_is_not_a_string = function()
        local ok, err = pcall(function()
            return (flags.defines({ "NDEBUG" }))
        end)
        assert(not ok, "expected an error")
        err = tostring(err)
        assert(err:find("c.defines", 1, true), err)
        assert(err:find("1", 1, true), err)
        assert(err:find(":%d+:"), "no location in " .. err)

        ok, err = pcall(function()
            return (flags.defines({ "NDEBUG", A = true }))
        end)
        assert(not ok, "expected an error")
        assert(tostring(err):find("c.defines", 1, true), tostring(err))
    end,

    shared_name_per_platform = function()
        assert(flags.shared_name("util", "linux") == "libutil.so")
        assert(flags.shared_name("util", "macos") == "libutil.dylib")
        assert(flags.shared_name("util", nil) == nil)
    end,

    soname_per_platform = function()
        same(flags.soname("libutil.so", "linux"), { "-Wl,-soname,libutil.so" })
        same(flags.soname("libutil.dylib", "macos"), { "-install_name", "@rpath/libutil.dylib" })
        same(flags.soname("libutil.so", nil), {})
    end,

    rpath_per_platform = function()
        same(flags.rpath("linux"), { "-Wl,-rpath,$ORIGIN" })
        same(flags.rpath("macos"), { "-Wl,-rpath,@loader_path" })
        same(flags.rpath(nil), {})
    end,

    an_unknown_platform_is_refused = function()
        refuses(function(p)
            return (flags.shared_name("util", p))
        end, "c.shared_name", "windows")
        refuses(function(p)
            return (flags.soname("libutil.so", p))
        end, "c.soname", "windows")
        refuses(flags.rpath, "c.rpath", "windows")
    end,

    platform_of_reads_triples = function()
        assert(flags.platform_of("arm64-apple-darwin25.3.0") == "macos")
        assert(flags.platform_of("aarch64-linux-gnu") == "linux")
        assert(flags.platform_of("x86_64-pc-linux-gnu") == "linux")
        refuses(flags.platform_of, "c.platform", "riscv64-unknown-elf")
    end,
}
