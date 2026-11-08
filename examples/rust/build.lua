local rust = buildl.use("rust")

-- The default profile, dev, keeps the absolute path of the folder it was built in as debug
-- info, so `buildl verify` would find the binary differing each time. release holds no path.
rust.binary("hello", { profile = "release" })
rust.test("unit")
