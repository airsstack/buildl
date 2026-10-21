local b = buildl
b.target("plain", { inputs = { "a.txt" }, run = { "cp", "$in", "$out" } })        -- a file, copied
b.target("copied", {                                                             -- reads what plain built
  deps   = { "plain" },
  inputs = { "copy.sh" },
  run    = { "sh", "$in", "$deps", "$out" },
})
b.target("site", {                                                               -- a directory output
  inputs = { "assets/one.txt", "assets/sub/two.txt" },
  run    = { "cp", "-R", "assets", "$out" },
})
b.target("stamp", { inputs = { "stamp.sh" }, run = { "sh", "$in", "$out" } })   -- differs on every run
