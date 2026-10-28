# Under the Hood

This chapter is for the curious, and for anyone who wants to work on `buildl` itself. It explains
how the pieces you have used fit together, why they are shaped the way they are, and, just as
importantly, what `buildl` can and cannot promise.

You do not need any of it to use `buildl`.

| Section | What it covers |
|---|---|
| [Declare, Then Execute](ch11-01-declare-then-execute.md) | the five phases of a build, the value each hands to the next, and the scheduler |
| [Crash Safety and the Store](ch11-02-crash-safety.md) | the life of one action, and why a build stopped half way never leaves a broken cache |
| [Four Crates, Ports and Adapters](ch11-03-four-crates.md) | how the code is split, and the rules that keep the logic of a build testable |
| [Errors](ch11-04-errors.md) | how a failure is classified, and how that becomes an exit code |
| [Determinism Rules](ch11-05-determinism-rules.md) | the engineering rules that keep the same inputs giving the same bytes |
| [Decision Records](ch11-06-decision-records.md) | each choice that was settled, with its reason |
| [Testing Layers](ch11-07-testing-layers.md) | which kind of test lives where |
| [Where buildl Sits](ch11-08-where-buildl-sits.md) | `buildl` placed among other build systems, in the terms of build-system theory |
| [What buildl Can and Cannot Promise](ch11-09-what-it-promises.md) | what is out of scope, and what each isolation tier does not guarantee |

The pages name real types, modules and files. Where an older design note and the code disagree,
these pages follow the code.
