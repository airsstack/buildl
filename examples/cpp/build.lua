-- A shared C++ library, a binary that loads it and a test that does too, through the
-- `c` plugin. The platform is known only after the first declaration pass, so every
-- value taken from it is empty on that pass and set on the second.
local c = buildl.use("c")

local platform = c.platform() -- nil on the first pass
local lib = c.shared_name("greet", platform) -- nil on the first pass

local headers = buildl.sources("include/*.h")

local cxxflags = c.includes({ "include" })
cxxflags[#cxxflags + 1] = "-fPIC"

c.shared_library("greet", {
  srcs = { "lib/greet.cc" },
  headers = headers,
  cxxflags = cxxflags,
  ldflags = c.soname(lib, platform),
  output = lib,
})

c.binary("hello", {
  srcs = { "src/main.cc" },
  headers = headers,
  cxxflags = cxxflags,
  ldflags = c.rpath(platform),
  deps = { ":greet" },
})

c.test("greet_test", {
  srcs = { "test/greet_test.cc" },
  headers = headers,
  cxxflags = cxxflags,
  ldflags = c.rpath(platform),
  deps = { ":greet" },
})
