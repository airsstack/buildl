local c = buildl.use("c")

-- A generated header. A target that reads it lists this one in header_deps; the
-- compiler finds it as out/config.h, hence -Iout below.
buildl.target("config.h", {
  run = { "sh", "-c", [[printf '#define GREETING "hello from buildl"\n' > "$0"]], "$out" },
})

local headers = buildl.sources("include/*.h")

local cflags = c.includes({ "include" })
cflags[#cflags + 1] = "-Iout"
for _, flag in ipairs(c.defines({ LEFT = 2, RIGHT = 3 })) do
  cflags[#cflags + 1] = flag
end

c.library("util", {
  srcs = { "lib/util.c" },
  headers = headers,
  cflags = cflags,
  output = "libutil.a",
})

c.binary("hello", {
  srcs = { "src/main.c" },
  headers = headers,
  header_deps = { ":config.h" },
  cflags = cflags,
  deps = { ":util", ":blob.o" },
})

c.test("util_test", {
  srcs = { "test/util_test.c" },
  headers = headers,
  cflags = cflags,
  deps = { ":util" },
})

-- src/blob.S is not C, so the plugin does not compile it. The build file does what
-- the plugin does for a C source: a discovery target runs the compiler with -MM, and
-- the object's inputs are what it names.
buildl.discover("blob.deps", {
  run = { "cc", "-Iinclude", "-MM", "-MT", "obj", "src/blob.S" },
  inputs = { "src/blob.S", headers },
  stdout = true,
})

-- The first pass runs before the discovery has: there is nothing to declare from yet.
local depfile = buildl.discovered(":blob.deps")
if depfile == nil then
  return
end

local inputs = {}
for _, rule in ipairs(c.depfile.parse(depfile)) do
  if rule.target == "obj" then
    inputs = rule.prerequisites
  end
end

buildl.target("blob.o", {
  run = { "cc", "-Iinclude", "-c", "-o", "$out", "src/blob.S" },
  inputs = inputs,
})
