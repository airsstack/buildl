# Sample

A checked block:

```lua
buildl.target("first", {
    run = { "true" },
    outputs = { "first.txt" },
})
```

A block left out on purpose:

```lua no-check
buildl.target("skipped", { input = { "a" } })
```

Other languages:

```toml
[workspace]
name = "toml-body"
```

```text
text-body
```

A second checked block:

```lua
buildl.target("second", {
    run = { "true" },
    outputs = { "second.txt" },
})
```
