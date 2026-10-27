local b = buildl
b.target("greeting", { run = { "printf", "hello\n" }, stdout = true })
b.discover("pkgs", {
  run = { "printf", "{\"name\":\"alpha\"}\n{\"name\":\"beta\"}\n" },
  stdout = true,
})
