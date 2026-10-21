local b = buildl
b.target("visible", { inputs = { "a.txt" }, run = { "cp", "$in", "$out" } })
b.target("hidden",  { inputs = { "a.txt" }, run = { "cp", "secret.txt", "$out" } }) -- reads a file it did not declare
b.target("partial", { inputs = { "partial.sh" }, run = { "sh", "$in", "$out" } })    -- writes, then fails
b.target("slow",    { inputs = { "slow.sh" }, run = { "sh", "$in", "$out" } })       -- writes, then outlasts the timeout
