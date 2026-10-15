local b = buildl
b.target("hello", { inputs = { "say.sh", "hello.txt" }, run = { "sh", "$in", "$out" } })
b.target("quiet", { deps = { "hello" }, run = { "cp", "$deps", "$out" } })
b.alias("greeting", "hello")
