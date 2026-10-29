# Migrating npm scripts

The `buildl` skill holds the build-file and `buildl.toml` rules. This file only maps what the
`scripts` of a `package.json` do to what buildl does.

## What `compare` reads

Check 1 of `compare` reads recipe names from the old build itself. For `tool: "npm"` it reads
`package.json` in the project folder and takes the keys of `scripts`. With no `package.json`,
`compare` stops with an error and prints no table.

| Seen by check 1 | Example |
|---|---|
| every key of `scripts` | `build`, `test`, `lint` |
| a lifecycle hook, as a key of its own | `prebuild`, `posttest`, `prepare` |

| Not seen by check 1 | What to do |
|---|---|
| the `package.json` of a workspace package in a subfolder | read it yourself and map its scripts with `subdir`, below |
| scripts that npm runs without a `scripts` key (`npm install`, `npm ci`) | map them by hand; see the download row below |
| the commands inside a script | map them by hand, below |

## Mapping

| npm | buildl |
|---|---|
| a script that builds files (`build`, `bundle`) | `b.target(file, { inputs, run })` |
| a script that checks (`test`, `lint`, `typecheck`) | `b.test(name, { deps, run })`. It passes when the command exits 0 |
| `prebuild`, `postbuild` (npm runs them around `build`) | a target the main one lists in `deps`, or commands put in the main target. Each name is still a key to map or drop |
| `npm run a && npm run b` | two targets, or a target with `deps` |
| a command whose output is a file (`cmd > file`) | `stdout = true`, and no `>` |
| `npm install`, `npm ci`, anything that downloads | the same command with `network = true`, and `npm` in `proc.run`. A later target reads files, so declare the files the install makes in `outputs`; if that cannot be done, stop and ask the user. Do not set `strategy = "bare"` |
| a script in a package in another folder (`npm run build --workspace=pkg`, `cd pkg && npm run build`) | `b.subdir("pkg")` and a `build.lua` in that folder. Its targets are `//pkg:name` |
| `$NODE_ENV`, `process.env.TOKEN` read by a script | `env = { "NODE_ENV" }` on the target and `env.read = ["NODE_ENV"]` in the ceiling |
| `start`, `dev`, `watch`, `serve` (run until stopped) | the stop rule, last section |
| `publish`, `prepublishOnly`, `deploy`, `release`, `version` | the stop rule, last section |

npm runs a script through the shell. A command with `&&`, `|`, `>` or `$VAR` becomes
`{ "sh", "-c", "…" }` with `sh` in `proc.run`, or two targets. List `sh`, `npm` and `node` in
`old.programs`, and every program the scripts call: a ceiling entry the old build does not use
fails check 6. Name the real program in `run` (`node`, `tsc`, `esbuild`) and not `npm run`. For the
build command of a language, see the table in the `buildl` skill.

The copies start from what the project folder holds. Two rules for `skip`:

- A file the old build makes and `outputs` lists (`dist/app.js`) needs no entry: it is left out
  already. Do not list a folder the old build writes into (`dist`): the folder is then missing
  from the copies and the script's redirect fails. The script creates it (`mkdir -p dist`), and
  `mkdir` goes in `old.programs`.
- List `node_modules` only when `old.build` installs the packages first, such as
  `["sh", "-c", "npm ci && npm run build"]`, with `npm` in `old.programs` and `old.network` set
  to `true`. Without that step, a copy that lacks `node_modules` cannot run any script that uses
  a package, so leave it out of `skip`.

## Example

```json
{
  "name": "demo",
  "scripts": {
    "build": "mkdir -p dist && cat src/a.js src/b.js > dist/app.js",
    "test": "node --test",
    "deploy": "npm publish"
  }
}
```

```lua
local b = buildl

local src = b.sources("src/*.js")

b.target("app.js", {
  inputs = { src },
  run = { "cat", "$in" },
  stdout = true,
})

b.test("test", {
  inputs = { "package.json", src, b.sources("test/*.js") },
  run = { "node", "--test" },
})
```

```toml
[workspace]
name   = "demo"
entry  = "build.lua"
out    = "out"
config = 1

[ceiling]
proc.run = ["cat", "node"]
env.read = []
fs.read  = ["."]
fs.write = ["out", ".buildl"]
```

`compare` lists three keys: `build`, `test` and `deploy`. `deploy` publishes to a registry, so
the stop rule applies, and the user chose to keep it in `package.json`. A buildl output sits
under `out/`, named as the target is, `app.js`. A target name holds only letters, digits, `.`,
`-` and `_`, so it cannot be `dist/app.js`. This project has no packages, so `skip` is empty:

```json
{
  "tool": "npm",
  "old": {
    "build": ["npm", "run", "build"],
    "test": ["npm", "test"],
    "programs": ["sh", "npm", "node", "mkdir", "cat"],
    "env": [],
    "network": false
  },
  "recipes": {
    "build": { "target": "//:app.js" },
    "test": { "target": "//:test" },
    "deploy": { "dropped": "publishes to the registry; stays in package.json" }
  },
  "outputs": [ { "old": "dist/app.js", "new": "out/app.js" } ],
  "skip": []
}
```

A package in a folder:

```lua no-check
-- needs pkg/build.lua beside it, which the checked blocks do not have
local b = buildl
b.subdir("pkg")
```

## Recipes that are not build steps

A script that deploys, tags, publishes, runs until stopped, or otherwise acts outside building
files is neither dropped silently nor forced into a target. List them, ask the user whether
each stays in `package.json` or is dropped, and record the answer as `dropped` with its reason,
so check 1 adds up.

## C and C++

If a script compiles each `.c` or `.cc` to its own `.o` and links them (a `node-gyp` build, or a
`make` call), the per-file stop rule applies. See `make.md`, "C and C++ compiled one file at a
time": one whole-program target, its output judged by the tests.
