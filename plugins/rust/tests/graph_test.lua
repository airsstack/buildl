-- Tests of the target graph. Run from a folder holding the modules: airsl test --allow-read . .
local fake_buildl = require("fake_buildl")
local graph = require("graph")

local FILES = { "Cargo.toml", "Cargo.lock", "src/main.rs" }

-- Returns options as `options.check` fills them, with the fields of `given` over them.
local function filled(kind, name, given)
    local out = {
        args = {},
        deps = {},
        profile = "dev",
        profile_dir = "debug",
        output = kind == "binary" and name or nil,
        version_probe = false,
    }
    for key, value in pairs(given or {}) do
        out[key] = value
    end
    return out
end

-- Declares one call in a build file at `folder` ("" unless given) over `files` and returns the fake.
local function declare(kind, name, given, folder, files)
    local fake = fake_buildl.new(folder or "", nil, files or FILES)
    graph.declare(fake, kind, name, filled(kind, name, given))
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

-- Asserts the command of `target` is `sh -c <script>` with `$out` as `$0`, then `args`.
local function builds(target, script, args)
    local run = target.options.run
    assert(run[1] == "sh" and run[2] == "-c", "run starts " .. tostring(run[1]) .. " " .. tostring(run[2]))
    assert(run[3] == script, "script is\n" .. tostring(run[3]) .. "\nwant\n" .. script)
    assert(run[4] == "$out", "$0 is " .. tostring(run[4]))
    local rest = {}
    for i = 5, #run do
        rest[#rest + 1] = run[i]
    end
    same(rest, args or {}, "args")
end

-- Asserts the command of a library `target` is `sh -c <script> sh $out`, then `args`.
local function builds_library(target, script, args)
    local run = target.options.run
    assert(run[1] == "sh" and run[2] == "-c", "run starts " .. tostring(run[1]) .. " " .. tostring(run[2]))
    assert(run[3] == script, "script is\n" .. tostring(run[3]) .. "\nwant\n" .. script)
    assert(run[4] == "sh" and run[5] == "$out", "$0 and $1 are " .. tostring(run[4]) .. " " .. tostring(run[5]))
    local rest = {}
    for i = 6, #run do
        rest[#rest + 1] = run[i]
    end
    same(rest, args or {}, "args")
end

return {
    binary_runs_cargo_build_in_the_folder = function()
        local root = get(declare("binary", "hello"), "hello")
        assert(root.kind == "target")
        builds(
            root,
            [[cargo build --locked --bin hello --profile dev --target-dir target "$@" && cp target/debug/hello "$0"]]
        )
        same(root.options.outputs, { "hello" }, "outputs")

        local sub = get(declare("binary", "hello", { output = "tool" }, "rust"), "hello")
        builds(
            sub,
            [[(cd rust && cargo build --locked --bin hello --profile dev --target-dir target "$@") && cp rust/target/debug/hello "$0"]]
        )
        same(sub.options.outputs, { "tool" }, "outputs")
    end,

    library_copies_every_file_cargo_wrote = function()
        local static = get(declare("library", "mylib", {
            libraries = { { kind = "staticlib", output = "libmylib.a" } },
        }), "mylib")
        builds_library(
            static,
            [[o1="$1"; shift 1; cargo build --locked --lib --profile dev --target-dir target "$@" && ]]
                .. [[cp target/debug/libmylib.a "$o1"]]
        )
        same(static.options.outputs, { "libmylib.a" }, "outputs")

        local both = get(declare("library", "ffi", {
            libraries = {
                { kind = "cdylib", output = "libffi.dylib" },
                { kind = "staticlib", output = "libffi.a" },
            },
            args = { "-p", "ffi" },
        }, "rust"), "ffi")
        builds_library(
            both,
            [[o1="$1"; o2="$2"; shift 2; ]]
                .. [[(cd rust && cargo build --locked --lib --profile dev --target-dir target "$@") && ]]
                .. [[if [ -f rust/target/debug/libffi.so ]; then cp rust/target/debug/libffi.so "$o1"; ]]
                .. [[else cp rust/target/debug/libffi.dylib "$o1"; fi && ]]
                .. [[cp rust/target/debug/libffi.a "$o2"]],
            { "-p", "ffi" }
        )
        same(both.options.outputs, { "libffi.dylib", "libffi.a" }, "outputs")
    end,

    test_runs_cargo_test = function()
        local fake = declare("test", "unit")
        same(names(fake), { "unit" }, "names")
        local unit = get(fake, "unit")
        assert(unit.kind == "test", unit.kind)
        local run = unit.options.run
        same({ run[1], run[2] }, { "sh", "-c" }, "shell")
        assert(run[3] == [[cargo test --locked --profile dev --target-dir target "$@"]], run[3])
        assert(unit.options.outputs == nil, "a test has outputs")
        assert(#run == 4 and run[4] == "sh", "$0 is " .. tostring(run[4]))

        local sub = get(declare("test", "unit", nil, "rust"), "unit").options.run
        assert(sub[3] == [[(cd rust && cargo test --locked --profile dev --target-dir target "$@")]], sub[3])
    end,

    args_and_profile_reach_cargo = function()
        local given = { args = { "-p", "server", "--features", "a b" }, profile = "release", profile_dir = "release" }
        local binary = get(declare("binary", "hello", given), "hello")
        builds(
            binary,
            [[cargo build --locked --bin hello --profile release --target-dir target "$@" && cp target/release/hello "$0"]],
            { "-p", "server", "--features", "a b" }
        )

        local unit = get(declare("test", "unit", given), "unit").options.run
        assert(unit[3] == [[cargo test --locked --profile release --target-dir target "$@"]], unit[3])
        same({ unit[5], unit[6], unit[7], unit[8] }, given.args, "test args")

        local fast = { profile = "it's", profile_dir = "it's" }
        local quoted = get(declare("binary", "hello", fast), "hello").options.run[3]
        assert(quoted:find([[--profile 'it'\''s']], 1, true), quoted)
        assert(quoted:find([[cp 'target/it'\''s/hello' "$0"]], 1, true), quoted)
    end,

    inputs_leave_out_target = function()
        local files = {
            "Cargo.toml",
            "Cargo.lock",
            "build.rs",
            "data.txt",
            "src/a/b.rs",
            "target/debug/x.rs",
            "target/CACHEDIR.TAG",
            ".git/HEAD",
            ".git/objects/ab/cdef",
        }
        local want = { "Cargo.toml", "Cargo.lock", "build.rs", "data.txt", "src/a/b.rs" }
        local binary = declare("binary", "hello", nil, "", files)
        same(get(binary, "hello").options.inputs, want, "inputs")
        same(binary.patterns, { "**/*" }, "patterns")
        same(get(declare("test", "unit", nil, "rust", files), "unit").options.inputs, want, "test inputs")

        local near = { "Cargo.toml", "Cargo.lock", "targets/x.rs", "target.txt", ".github/ci.yml", ".gitignore" }
        same(get(declare("binary", "hello", nil, "", near), "hello").options.inputs, near, "near")
    end,

    env_and_network = function()
        for _, kind in ipairs({ "binary", "library", "test" }) do
            local given = kind == "library" and { libraries = { { kind = "staticlib", output = "libx.a" } } } or nil
            local target = get(declare(kind, "x", given), "x").options
            same({ target.env.HOME, target.env.PATH }, { "unkeyed", "unkeyed" }, kind .. " env")
            local count = 0
            for _ in pairs(target.env) do
                count = count + 1
            end
            assert(count == 2, kind .. " env has " .. count .. " names")
            assert(target.network == true, kind .. " network")
        end
        local binary = get(declare("binary", "x", { deps = { ":gen", "//lib:cfg" } }), "x")
        same(binary.options.deps, { ":gen", "//lib:cfg" }, "deps")
        same(get(declare("test", "t", { deps = { ":gen" } }), "t").options.deps, { ":gen" }, "test deps")
    end,

    missing_lockfile_or_manifest_is_refused = function()
        local err = refuses(function()
            return declare("binary", "hello", nil, "", { "Cargo.toml", "src/main.rs" })
        end, { "rust.binary(hello): no Cargo.lock beside Cargo.toml; run cargo generate-lockfile" })
        assert(err:find(":%d+:"), "no location in " .. err)

        refuses(function()
            return declare("test", "unit", nil, "rust", { "Cargo.lock", "src/main.rs" })
        end, { "rust.test(unit): no Cargo.toml in rust" })
        refuses(function()
            return declare("library", "lib", { libraries = { { kind = "staticlib", output = "liblib.a" } } }, "", {})
        end, { "rust.library(lib): no Cargo.toml in ." })

        local fake = fake_buildl.new("", nil, { "Cargo.toml" })
        pcall(graph.declare, fake, "binary", "hello", filled("binary", "hello", { version_probe = true }))
        same(names(fake), {}, "declared before the refusal")
    end,

    version_probe_adds_a_dependency = function()
        local off = declare("binary", "hello", { deps = { ":gen" } })
        same(names(off), { "hello" }, "no probe")

        local fake = declare("binary", "hello", { version_probe = true, deps = { ":gen" } })
        local probe = get(fake, "hello.version")
        assert(probe.kind == "target")
        same(probe.options.run, { "rustc", "-vV" }, "run")
        assert(probe.options.stdout == true and probe.options.always == true, "stdout and always")
        same(get(fake, "hello").options.deps, { ":gen", ":hello.version" }, "deps")

        local test = declare("test", "unit", { version_probe = true })
        same(get(test, "unit").options.deps, { ":unit.version" }, "test deps")
    end,
}
