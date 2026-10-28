local gobin = buildl.use("gobin")

-- Each call declares one target building a Go program of this module.
gobin.binary("hello", "hello")
gobin.binary("bye", "bye")
