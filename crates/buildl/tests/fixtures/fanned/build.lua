buildl.subdir("a")
buildl.subdir("b")
buildl.subdir("c")
buildl.target("copy", { inputs = { "in.txt" }, run = { "cp", "$in", "$out" } })
