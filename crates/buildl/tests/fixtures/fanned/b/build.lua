buildl.subdir("y")
buildl.target("copy", { inputs = { "in.txt" }, run = { "cp", "$in", "$out" } })
