local c = buildl.use("c")

-- The link flags depend on the platform, which the first pass does not know yet: c.platform()
-- is nil until its discovery has run. The calls below are made on both passes all the same,
-- since the plugin declares its own discovery targets on the first one; only the link flags
-- wait for the platform.
local platform = c.platform()

local headers = buildl.sources("include/*.h")
local cflags = c.includes({ "include" })

-- A Rust static library holds the Rust standard library, which needs these system libraries
-- at link time on Linux (`rustc --print native-static-libs`); on macOS cc links them already.
local static_libs = {}
if platform == "linux" then
  static_libs = { "-lgcc_s", "-lutil", "-lrt", "-lpthread", "-lm", "-ldl", "-lc" }
end

-- A C library Rust calls: //rust:calc depends on it and links out/c/libclamp.a.
c.library("clamp", {
  srcs = { "lib/clamp.c" },
  headers = headers,
  cflags = cflags,
  output = "libclamp.a",
})

c.binary("app_static", {
  srcs = { "src/static_main.c" },
  headers = headers,
  cflags = cflags,
  deps = { "//rust:mathlib" },
  ldlibs = static_libs,
})

-- The binary lands in out/c/, the shared library in out/rust/. c.rpath gives the flag for the
-- binary's own folder ($ORIGIN or @loader_path), {} before the platform is known; the build
-- file points it one folder over.
local rpath = c.rpath(platform)
if rpath[1] ~= nil then
  rpath[1] = rpath[1] .. "/../rust"
end

c.binary("app_shared", {
  srcs = { "src/shared_main.c" },
  headers = headers,
  cflags = cflags,
  ldflags = rpath,
  deps = { "//rust:greeter" },
})

c.test("mathlib_test", {
  srcs = { "test/mathlib_test.c" },
  headers = headers,
  cflags = cflags,
  deps = { "//rust:mathlib" },
  ldlibs = static_libs,
})
