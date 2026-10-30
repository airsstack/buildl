-- Tests of the target graph. Run from a folder holding the modules: airsl test --allow-read . .
local fake_buildl = require("fake_buildl")
local graph = require("graph")
local options = require("options")

-- Declares one call in a build file at `folder` ("cpp" unless given) and returns the fake.
local function declare(kind, name, given, results, folder)
    local fake = fake_buildl.new(folder or "cpp", results)
    graph.declare(fake, kind, name, options.check(kind, name, given))
    return fake
end

local function same(got, want, what)
    what = what or "list"
    assert(type(got) == "table", what .. " is " .. tostring(got) .. ", want a list")
    assert(#got == #want, what .. " has " .. #got .. " items, want " .. #want .. ": " .. table.concat(got, " "))
    for i = 1, #want do
        assert(got[i] == want[i], what .. " item " .. i .. " is " .. tostring(got[i]) .. ", want " .. want[i])
    end
end

local function deep_equal(a, b)
    if type(a) ~= "table" or type(b) ~= "table" then
        return a == b
    end
    for key, value in pairs(a) do
        if not deep_equal(value, b[key]) then
            return false
        end
    end
    for key in pairs(b) do
        if a[key] == nil then
            return false
        end
    end
    return true
end

local function names(fake)
    local found = {}
    for _, declaration in ipairs(fake.declared) do
        found[#found + 1] = declaration.name
    end
    return found
end

local function get(fake, name)
    local declaration = fake.find(name)
    assert(declaration, "no target " .. name .. " in " .. table.concat(names(fake), " "))
    return declaration
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

local A = { srcs = { "src/a.c" }, headers = { "include/a.h" }, cflags = { "-Iinclude" } }
local A_FOUND = { [":app.src.a.c.deps"] = "obj: cpp/src/a.c cpp/include/a.h\n" }

return {
    first_pass_declares_only_discovery = function()
        local fake = declare("binary", "app", { srcs = { "src/a.c", "src/b.cc" }, headers = { "include/a.h" } })
        same(names(fake), { "app.src.a.c.deps", "app.src.b.cc.deps" }, "names")
        for _, declaration in ipairs(fake.declared) do
            assert(declaration.kind == "discover", declaration.name .. " is a " .. declaration.kind)
        end
    end,

    discovery_runs_the_compiler_on_the_root_path = function()
        local fake = declare("binary", "app", A)
        local found = get(fake, "app.src.a.c.deps").options
        same(found.run, { "cc", "-Iinclude", "-MM", "-MT", "obj", "cpp/src/a.c" }, "run")
        same(found.inputs, { "src/a.c", "include/a.h" }, "inputs")
        same(found.deps, {}, "deps")
        assert(found.stdout == true, "stdout")

        local root = declare("binary", "app", { srcs = { "a.c" } }, nil, "")
        same(get(root, "app.a.c.deps").options.run, { "cc", "-MM", "-MT", "obj", "a.c" }, "run at the root")
    end,

    discovery_commands_are_the_same_on_both_passes = function()
        local given = {
            srcs = { "src/a.c", "src/b.cpp" },
            headers = { "include/a.h" },
            header_deps = { ":gen" },
            cflags = { "-O2" },
            cxxflags = { "-std=c++17" },
            cc = "gcc",
            cxx = "g++",
            ldflags = { "-L." },
        }
        local first = declare("binary", "app", given)
        local second = declare("binary", "app", given, {
            [":app.src.a.c.deps"] = "obj: cpp/src/a.c cpp/include/a.h",
            [":app.src.b.cpp.deps"] = "obj: cpp/src/b.cpp",
        })
        local count = 0
        for _, declaration in ipairs(first.declared) do
            assert(declaration.kind == "discover")
            assert(deep_equal(declaration, second.find(declaration.name)), declaration.name .. " differs")
            count = count + 1
        end
        assert(count == 2, "discovery targets: " .. count)
        assert(#second.declared > count, "the second pass declares more")
    end,

    an_object_takes_the_headers_it_read = function()
        local fake = declare("binary", "app", A, A_FOUND)
        local object = get(fake, "app.src.a.c.o")
        assert(object.kind == "target")
        same(object.options.inputs, { "src/a.c", "include/a.h" }, "inputs")
        same(object.options.run, { "cc", "-Iinclude", "-c", "-o", "$out", "cpp/src/a.c" }, "run")
        same(object.options.deps, {}, "deps")
    end,

    an_object_keeps_only_the_obj_rule_and_skips_the_source = function()
        local fake = declare("binary", "app", A, {
            [":app.src.a.c.deps"] = "other.o: cpp/other.c cpp/include/b.h\nobj: \\\n cpp/src/a.c \\\n cpp/include/a.h\n",
        })
        same(get(fake, "app.src.a.c.o").options.inputs, { "src/a.c", "include/a.h" }, "inputs")
    end,

    a_dotdot_header_is_normalised = function()
        local fake = declare("binary", "app", A, { [":app.src.a.c.deps"] = "obj: cpp/src/a.c cpp/src/../include/a.h" })
        same(get(fake, "app.src.a.c.o").options.inputs, { "src/a.c", "include/a.h" }, "inputs")
    end,

    a_header_above_the_root_is_refused = function()
        refuses(function()
            return (declare("binary", "app", A, { [":app.src.a.c.deps"] = "obj: cpp/src/a.c ../../x.h" }))
        end, { "c.binary(app)", "../../x.h" })
    end,

    an_absolute_header_is_refused_with_the_isystem_hint = function()
        local err = refuses(function()
            return (declare("binary", "app", A, { [":app.src.a.c.deps"] = "obj: cpp/src/a.c /opt/x/y.h" }))
        end, { "c.binary(app)", "/opt/x/y.h", "-isystem" })
        assert(err:find(":%d+:"), "no location in " .. err)
    end,

    a_header_outside_headers_is_refused = function()
        refuses(function()
            return (declare("binary", "app", A, { [":app.src.a.c.deps"] = "obj: cpp/src/a.c cpp/include/b.h" }))
        end, { "c.binary(app)", "cpp/include/b.h", "one build file per project" })
        refuses(function()
            return (declare("binary", "app", A, { [":app.src.a.c.deps"] = "obj: cpp/src/a.c lib/c.h" }))
        end, { "lib/c.h", "one build file per project" })
    end,

    a_dependency_output_is_left_to_header_deps = function()
        local given = { srcs = { "src/a.c" }, headers = { "include/a.h" }, header_deps = { ":gen", "//lib:cfg" } }
        local fake = declare("binary", "app", given, {
            [":app.src.a.c.deps"] = "obj: cpp/src/a.c cpp/include/a.h out/cpp/config.h",
        })
        local object = get(fake, "app.src.a.c.o").options
        same(object.inputs, { "src/a.c", "include/a.h" }, "inputs")
        same(object.deps, { ":gen", "//lib:cfg" }, "deps")
        same(get(fake, "app.src.a.c.deps").options.deps, { ":gen", "//lib:cfg" }, "discovery deps")
    end,

    a_result_without_obj_is_refused = function()
        refuses(function()
            return (declare("binary", "app", A, { [":app.src.a.c.deps"] = "other.o: cpp/src/a.c" }))
        end, { "c.binary(app)", "src/a.c" })
    end,

    cxx_sources_use_cxx_and_cxxflags = function()
        local given = { srcs = { "src/a.cpp" }, cflags = { "-DC" }, cxxflags = { "-std=c++17" } }
        local fake = declare("binary", "app", given, { [":app.src.a.cpp.deps"] = "obj: cpp/src/a.cpp" })
        same(
            get(fake, "app.src.a.cpp.deps").options.run,
            { "c++", "-std=c++17", "-MM", "-MT", "obj", "cpp/src/a.cpp" },
            "discovery"
        )
        same(
            get(fake, "app.src.a.cpp.o").options.run,
            { "c++", "-std=c++17", "-c", "-o", "$out", "cpp/src/a.cpp" },
            "compile"
        )
    end,

    names_keep_the_extension = function()
        local fake = declare("binary", "app", { srcs = { "src/main.c", "src/main.cc" } }, {
            [":app.src.main.c.deps"] = "obj: cpp/src/main.c",
            [":app.src.main.cc.deps"] = "obj: cpp/src/main.cc",
        })
        get(fake, "app.src.main.c.o")
        get(fake, "app.src.main.cc.o")
        same(get(fake, "app").options.deps, { ":app.src.main.c.o", ":app.src.main.cc.o" }, "deps")
    end,

    binary_links_objects_then_deps_with_path = function()
        local given = {
            srcs = { "src/a.c", "src/b.c" },
            ldflags = { "-L." },
            ldlibs = { "-lm" },
            deps = { ":lib" },
        }
        local fake = declare("binary", "app", given, {
            [":app.src.a.c.deps"] = "obj: cpp/src/a.c",
            [":app.src.b.c.deps"] = "obj: cpp/src/b.c",
        })
        local link = get(fake, "app")
        assert(link.kind == "target")
        same(link.options.deps, { ":app.src.a.c.o", ":app.src.b.c.o", ":lib" }, "deps")
        same(link.options.run, { "cc", "-L.", "-o", "$out", "$deps", "-lm" }, "run")
        same(link.options.env, { "PATH" }, "env")
        same(link.options.outputs, { "app" }, "outputs")
    end,

    a_linker_and_an_output_can_be_chosen = function()
        local fake = declare("binary", "app", { srcs = { "a.c" }, linker = "gcc", output = "tool" }, {
            [":app.a.c.deps"] = "obj: cpp/a.c",
        })
        local link = get(fake, "app").options
        assert(link.run[1] == "gcc", link.run[1])
        same(link.outputs, { "tool" }, "outputs")
    end,

    shared_library_adds_only_shared = function()
        local given = { srcs = { "a.c" }, ldflags = { "-fPIC" }, ldlibs = { "-lm" } }
        local fake = declare("shared_library", "greet", given, { [":greet.a.c.deps"] = "obj: cpp/a.c" })
        local link = get(fake, "greet").options
        same(link.run, { "cc", "-fPIC", "-shared", "-o", "$out", "$deps", "-lm" }, "run")
        same(link.env, { "PATH" }, "env")
        same(link.deps, { ":greet.a.c.o" }, "deps")

        local bare = declare("shared_library", "greet", { srcs = { "a.c" } }, { [":greet.a.c.deps"] = "obj: cpp/a.c" })
        same(get(bare, "greet").options.run, { "cc", "-shared", "-o", "$out", "$deps" }, "bare run")
    end,

    library_archives_with_zeroed_dates = function()
        local fake = declare("library", "lib", { srcs = { "a.c", "b.c" } }, {
            [":lib.a.c.deps"] = "obj: cpp/a.c",
            [":lib.b.c.deps"] = "obj: cpp/b.c",
        })
        local archive = get(fake, "lib").options
        same(archive.run, { "sh", "-c", 'ZERO_AR_DATE=1 exec ar rcs "$0" "$@"', "$out", "$deps" }, "run")
        same(archive.deps, { ":lib.a.c.o", ":lib.b.c.o" }, "deps")
        same(archive.outputs, { "lib" }, "outputs")
        same(archive.env, { "PATH" }, "env")
    end,

    test_without_deps_runs_the_binary = function()
        local fake = declare("test", "unit", { srcs = { "t.c" } }, { [":unit.t.c.deps"] = "obj: cpp/t.c" })
        local binary = get(fake, "unit.bin")
        assert(binary.kind == "target")
        same(binary.options.outputs, { "unit.bin" }, "outputs")
        same(binary.options.env, { "PATH" }, "env")
        local test = get(fake, "unit")
        assert(test.kind == "test")
        same(test.options.deps, { ":unit.bin" }, "deps")
        same(test.options.run, { "$deps" }, "run")
    end,

    test_with_deps_runs_only_the_binary = function()
        local fake = declare("test", "unit", { srcs = { "t.c" }, deps = { ":lib" } }, {
            [":unit.t.c.deps"] = "obj: cpp/t.c",
        })
        same(get(fake, "unit.bin").options.deps, { ":unit.t.c.o", ":lib" }, "binary deps")
        local test = get(fake, "unit").options
        same(test.deps, { ":unit.bin", ":lib" }, "deps")
        same(test.run, { "sh", "-c", 'exec "$0"', "$deps" }, "run")
    end,

    version_probe_is_off_by_default = function()
        local fake = declare("binary", "app", A, A_FOUND)
        for _, name in ipairs(names(fake)) do
            assert(not name:find(".version.", 1, true), name)
        end
    end,

    version_probe_adds_a_dependency_of_each_compile = function()
        local given = { srcs = { "a.c", "b.cpp" }, version_probe = true, header_deps = { ":gen" } }
        local first = declare("binary", "app", given)
        same(names(first), { "app.version.c", "app.version.cpp", "app.a.c.deps", "app.b.cpp.deps" }, "first pass")

        local fake = declare("binary", "app", given, {
            [":app.a.c.deps"] = "obj: cpp/a.c",
            [":app.b.cpp.deps"] = "obj: cpp/b.cpp",
        })
        local c_probe = get(fake, "app.version.c")
        same(c_probe.options.run, { "cc", "--version" }, "C probe")
        assert(c_probe.options.always == true and c_probe.options.stdout == true, "C probe always and stdout")
        local cpp_probe = get(fake, "app.version.cpp")
        same(cpp_probe.options.run, { "c++", "--version" }, "C++ probe")
        assert(cpp_probe.options.always == true and cpp_probe.options.stdout == true, "C++ probe always and stdout")

        same(get(fake, "app.a.c.o").options.deps, { ":gen", ":app.version.c" }, "C object deps")
        same(get(fake, "app.b.cpp.o").options.deps, { ":gen", ":app.version.cpp" }, "C++ object deps")
        for _, name in ipairs(names(fake)) do
            assert(not name:find("+", 1, true), name)
        end
    end,

    a_probe_is_declared_only_for_a_language_the_call_compiles = function()
        local fake = declare("binary", "app", { srcs = { "a.c" }, version_probe = true })
        get(fake, "app.version.c")
        assert(fake.find("app.version.cpp") == nil, "no C++ probe")
    end,
}
