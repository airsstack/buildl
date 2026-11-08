-- Tests of the plugin's public table. Run from a folder holding the modules: airsl test --allow-read . .
local api = require("api")
local fake_buildl = require("fake_buildl")

-- The files a build file in the folder has, enough for a declaration to pass its checks.
local FILES = { "Cargo.toml", "Cargo.lock", "src/main.rs" }

-- Returns the public table over a fresh fake whose `folder()` is `folder` ("" unless given).
local function plugin(results, folder)
    local fake = fake_buildl.new(folder or "", results, FILES)
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

-- Runs `run`, expects it to raise, checks the message holds each of `words` and names the
-- caller's file and not the plugin's; returns the message.
local function refuses(run, words)
    local ok, err = pcall(run)
    assert(not ok, "expected an error")
    err = tostring(err)
    for _, word in ipairs(words) do
        assert(err:find(word, 1, true), word .. " is not in " .. err)
    end
    assert(err:find("api_test", 1, true), "the location is not this file's: " .. err)
    assert(not err:find("api.lua", 1, true), "the location is the plugin's: " .. err)
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
        local rust = plugin()
        local want = { "binary", "library", "test", "package", "features", "platform", "shared_name" }
        local seen = {}
        for _, key in ipairs(want) do
            assert(type(rust[key]) == "function", "the table has no function " .. key)
            seen[key] = true
        end
        for key in pairs(rust) do
            assert(seen[key], "the table holds " .. tostring(key) .. ", which is not public")
        end
    end,

    a_function_returns_nothing = function()
        local rust, fake = plugin()
        assert(rust.binary("app") == nil)
        assert(rust.library("lib", { kind = "staticlib" }) == nil)
        assert(rust.test("unit") == nil)
        assert(fake.find("app").kind == "target")
        assert(fake.find("lib").kind == "target")
        assert(fake.find("unit").kind == "test")
    end,

    an_error_names_the_build_files_line = function()
        local rust = plugin()
        local err = refuses(function()
            rust.binary("app", { bogus = true })
        end, { "rust.binary(app)", "the option bogus is not known" })
        assert(err:find(":%d+:"), "no location in " .. err)
        refuses(function()
            rust.library("lib")
        end, { "rust.library(lib)", 'kind must be "staticlib" or "cdylib"' })

        -- An error of the graph, not of the options.
        local bare = api.new(fake_buildl.new("", nil, { "Cargo.toml" }))
        refuses(function()
            bare.test("unit")
        end, { "rust.test(unit)", "no Cargo.lock beside Cargo.toml; run cargo generate-lockfile" })
    end,

    package_and_features_return_lists = function()
        local rust, fake = plugin()
        same(rust.package("server"), { "-p", "server" }, "package")
        same(rust.features({ "tls", "json" }), { "--features", "tls,json" }, "features")
        same(rust.features({}), {}, "no features")
        assert(#fake.declared == 0, "a helper declared something")
        refuses(function()
            rust.package(7)
        end, { "rust.package: the name must be a string" })
        refuses(function()
            rust.package()
        end, { "rust.package: the name must be a string" })
        refuses(function()
            rust.features("tls")
        end, { "rust.features: the features must be a list of strings" })
        refuses(function()
            rust.features({ "tls", 7 })
        end, { "rust.features: the features must be a list of strings" })
    end,

    platform_is_nil_then_known = function()
        local rust, fake = plugin()
        assert(rust.platform() == nil)
        assert(rust.platform() == nil)
        assert(count(fake, "rust.platform") == 1, "rust.platform is not declared once")
        local found = fake.find("rust.platform")
        assert(found.kind == "discover")
        same(found.options.run, { "rustc", "-vV" }, "run")
        assert(found.options.stdout == true, "stdout is not set")

        local output = "rustc 1.99.0 (abc 2026-01-01)\nbinary: rustc\nhost: %s\nrelease: 1.99.0\n"
        local mac, mac_fake = plugin({ [":rust.platform"] = output:format("aarch64-apple-darwin") })
        assert(mac.platform() == "macos")
        assert(mac.platform() == "macos")
        assert(count(mac_fake, "rust.platform") == 1, "rust.platform is declared more than once")

        local linux = plugin({ [":rust.platform"] = output:format("x86_64-unknown-linux-gnu") })
        assert(linux.platform() == "linux")

        local other = plugin({ [":rust.platform"] = output:format("x86_64-pc-windows-msvc") })
        refuses(function()
            other.platform()
        end, {
            "rust.platform: the target triple x86_64-pc-windows-msvc is neither macos nor linux",
        })
    end,

    shared_name_by_platform = function()
        local rust = plugin()
        assert(rust.shared_name("ffi", "linux") == "libffi.so")
        assert(rust.shared_name("ffi", "macos") == "libffi.dylib")
        assert(rust.shared_name("ffi", nil) == nil)
    end,
}
