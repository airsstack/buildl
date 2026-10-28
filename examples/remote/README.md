# Remote cache and execution

The Python starter's sources with a `[remote]` table, and a NativeLink server to point it at.
A target runs on the server, a second checkout fetches the result, and a change built here is
uploaded for the next checkout to fetch.

```text
            ┌──────────── compose.yaml: NativeLink, 127.0.0.1:50051 ────────────┐
            │  CAS + action cache + scheduler + one local worker with python3  │
            └───────▲─────────────────────────▲─────────────────────────▲───────┘
  A --remote-execute│ runs there    B         │ fetches    C --remote-upload│ sends   D fetches
         1 remote                    1 fetched             1 built            1 fetched
```

## What it uses

| Piece | Why |
|---|---|
| `[remote] url` in `buildl.toml` | the server to share action results through; with no flag, `build` looks up there a target the local cache cannot serve and reports a hit as `fetched` |
| `--remote-execute` | a target neither cache serves runs on the server's worker, reported as `remote` |
| `--remote-upload` | what ran here is sent to the server's cache, for another checkout to fetch |
| `--remote-strict` | a failed remote call fails the build, instead of the build going on locally |
| `docker/Dockerfile` | NativeLink's static binary on a Debian base with `python3`: the worker runs each action as a process in the container, so the program it names must be there |
| `docker/nativelink.json5` | one process serving the CAS, the action cache, the scheduler and a worker that matches Linux x86-64 |

The server keeps its cache in the container, so `docker compose down` empties it. It asks for
no login and runs what it is sent, so `compose.yaml` publishes its port on `127.0.0.1` only.

## Where it runs

Needs the `buildl` binary, Python 3 and Docker. NativeLink's image is x86_64, and its worker
runs an action only for a client of the same OS family and instruction set:

| Client | Cache: `--remote-upload`, then `fetched` | Execution: `--remote-execute` |
|---|---|---|
| Linux x86_64 | yes; CI runs it | yes; CI runs it |
| macOS on Apple silicon, the server under Docker Desktop's x86_64 emulation | yes, tried by hand | no: no worker matches a macOS client, so the action waits in the server's queue (`queue_timeout`, 1 hour by default) |

Nothing is claimed for other machines.

## Run it on Linux x86_64

```sh
docker compose up -d --build      # wait for 'Worker registered with scheduler' in docker compose logs
sh assert.sh                      # the four copies A, B, C, D in temporary directories
docker compose down
```

By hand, against a server just started (one that already ran these actions answers `1 fetched`
to the first command), in two fresh copies of this directory:

```sh
buildl build --remote-execute     # copy 1: 1 remote
buildl test --remote-execute      # copy 1: tests: 1 passed (1 remote)
buildl build                      # copy 2: 1 fetched
```

`assert.sh` adds a line no earlier run wrote to `app/greet.py` in each copy, so the server's
cache never holds the actions already and it can run against a server that has served it
before. The repository's CI runs it in the `examples-remote` job.

## Run the cache half on a Mac

`assert.sh` runs execution too, so on a Mac run the cache half by hand, in two fresh copies of
this directory (each copy's own `.buildl` cache answers before the server is asked):

```sh
docker compose up -d --build                    # wait for 'Worker registered with scheduler'
buildl build --remote-strict --remote-upload    # copy 1: 1 built, and the result is sent
buildl build --remote-strict                    # copy 2: 1 fetched
docker compose down
```
