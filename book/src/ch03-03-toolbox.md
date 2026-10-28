# The Toolbox

Build files are often loops: one target per source file, a name worked out from a path, a list
filtered by a pattern. The `airsstack` table holds five helper modules for that work. They come
from [airsl](https://github.com/airsstack/airsl), the Lua runtime buildl evaluates build files on.

A build file can list them itself. This one sorts the names in `airsstack` and raises them as an
error, the only way a build file can show you something:

```lua
-- build.lua
local names = {}
for name in pairs(airsstack) do
  names[#names + 1] = name
end
table.sort(names)
error(table.concat(names, ", ") .. "; one table: " .. tostring(buildl == airsstack.buildl), 0)
```

```console
$ buildl check
error: build.lua: runtime error: lua error in build.lua: runtime error: buildl, glob, hash, json, path, regex; one table: true
stack traceback:
	[C]: in ?
	[C]: in function 'error'
	build.lua:6: in main chunk
```

Five modules, and `buildl` itself. In a build file they work only on the values you pass in: none
of them can read a file, the clock or the environment.

## One build file using all five

This build file turns each Markdown page in `pages/` into a target, skips the drafts, and writes an
index of what it built:

```lua
-- build.lua
local b = buildl
local glob, hash, json, path, regex =
  airsstack.glob, airsstack.hash, airsstack.json, airsstack.path, airsstack.regex

local pages = {}
for _, src in ipairs(b.sources("pages/*")) do
  local file = path.basename(src)
  if glob.match("*.md", file) and not regex.is_match("^draft-", file) then
    local name = path.stem(file) .. ".txt"
    b.target(name, { inputs = { src }, run = { "cp", "$in", "$out" } })
    pages[#pages + 1] = name
  end
end

local stamp = hash.sha256(table.concat(pages, "\n")):sub(1, 12)
b.target("index.json", {
  run    = { "echo", json.encode({ pages = pages, stamp = stamp }) },
  stdout = true,
})
```

With `pages/` holding `draft-ideas.md`, `guide.md`, `intro.md` and `notes.txt`:

```console
$ buildl build
--- //:index.json
{"pages":["guide.txt","intro.txt"],"stamp":"c00644b5f375"}
3 built
$ ls out
guide.txt
index.json
intro.txt
```

What each module did:

| Module | Call | Did |
|---|---|---|
| `path` | `path.basename(src)`, `path.stem(file)` | `pages/intro.md` to `intro.md`, then to `intro` |
| `glob` | `glob.match("*.md", file)` | kept the Markdown files, so `notes.txt` got no target |
| `regex` | `regex.is_match("^draft-", file)` | dropped `draft-ideas.md` |
| `hash` | `hash.sha256(text)` | a SHA-256 of the page list, as hex |
| `json` | `json.encode(value)` | a table to JSON text, keys sorted |

`json.decode` goes the other way, and is how a build file reads what a discovery target found
([Discovery Targets](ch07-00-discovery-targets.md)).

## What is held back

The modules are airsl's, with the parts that would reach outside the build file switched off:

| Function | In a build file |
|---|---|
| `glob.walk`, `hash.hash_file` | refused: they read the disk, and a build file holds no grant to read with |
| `path.absolute` | removed: it would answer differently depending on where you ran `buildl` |

A refused call is an error, never an empty answer, so a build file cannot mistake "not allowed to
look" for "nothing there":

```lua
-- build.lua
local found = airsstack.glob.walk(".", "*.md")
```

```console
$ buildl check
error: build.lua: operation refused: lua error in build.lua: glob.walk denied: `/private/tmp/buildl-book/toolbox-walk` is outside the granted read roots: none are granted
stack traceback:
	[C]: in field 'walk'
	build.lua:1: in main chunk

```

`buildl.sources` is the way to list files, as the build file above does. buildl does the walk
itself, inside the build file's directory, and sorts what it finds.

Every other function is as airsl documents it. The airsl book's
[module reference](https://github.com/airsstack/airsl/blob/main/book/src/appendix-02-modules.md)
lists them all.
