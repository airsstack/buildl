buildl.subdir("x")
buildl.target("copy", { inputs = { "in.txt" }, run = { "cp", "$in", "$out" } })
