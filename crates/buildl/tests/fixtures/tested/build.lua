local b = buildl
b.target("copy", { inputs = { "a.txt" }, run = { "cp", "$in", "$out" } })
b.test("same", { deps = { "copy" }, inputs = { "expected.txt" }, run = { "cmp", "$deps", "$in" } })
b.test("differs", { deps = { "copy" }, inputs = { "other.txt" }, run = { "cmp", "$deps", "$in" } })
b.test("unreported", { deps = { "copy" }, inputs = { "expected.txt" }, outputs = { "report.txt" }, run = { "cmp", "$deps", "$in" } })
b.test("flagged", { deps = { "copy" }, run = { "cmp", "$deps", "flag.txt" } })
