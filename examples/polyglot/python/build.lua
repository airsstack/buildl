local b = buildl

-- $in is python/pack.py, which python3 runs, then the tool's files; $deps is
-- every output of //schema:gen, of which pack.py keeps gen.py; $out is the archive.
b.target("tool.pyz", {
  inputs = { "pack.py", b.sources("tool/*.py") },
  deps = { "//schema:gen" },
  run = { "python3", "$in", "$deps", "$out" },
})
