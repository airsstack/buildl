# Python starter

A Python program and its test, built and run by buildl, with the standard library only.

| Target | What it does |
|---|---|
| `//:app.pyz` | `python3 -m zipapp app -o $out`, packing `app/` into one runnable archive, `out/app.pyz` |
| `//:unit` | `python3 -m unittest discover -s tests`, a test target |

Tests use `unittest` rather than pytest, which would need a virtual environment or `pip` in the
sandbox. `buildl.toml` lets the build run `python3` and read no environment variable.

## Run it

Needs the `buildl` binary and Python 3.

```sh
buildl build                  # builds out/app.pyz
python3 out/app.pyz           # hello from python
buildl test                   # runs //:unit
echo "# edit" >> app/greet.py
buildl plan                   # run    //:app.pyz: input app/greet.py changed
buildl test                   # runs //:unit again: app/greet.py is one of its inputs
buildl test                   # tests: 1 passed (1 cached)
buildl log //:unit            # what unittest printed when it last ran
```

From `examples/`, `sh check.sh python` runs these steps on a copy and checks each result
(`assert.sh`).
