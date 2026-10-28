local b = buildl

-- One target writes all four files, so every program depends on it. $in is
-- schema/generate.py then schema/schema.json, in the order written; $out is the
-- four outputs, under out/schema/.
b.target("gen", {
  inputs = { "generate.py", "schema.json" },
  outputs = { "gogen/gen.go", "gogen/go.mod", "gen.rs", "gen.py" },
  run = { "python3", "$in", "$out" },
})
