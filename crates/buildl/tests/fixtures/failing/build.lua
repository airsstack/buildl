local b = buildl
b.target("bad",    { run = { "false" } })
b.target("after",  { deps = { "bad" },   inputs = { "x.txt" }, run = { "cp", "$in", "$out" } })
b.target("after2", { deps = { "after" }, run = { "cp", "$deps", "$out" } })
b.target("good",   { inputs = { "x.txt" }, run = { "cp", "$in", "$out" } })
b.target("good2",  { deps = { "good" },  run = { "cp", "$deps", "$out" } })
