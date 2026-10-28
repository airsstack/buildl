# Caching

Run `buildl build` twice and the second run does nothing. That is not because `buildl` looked at
file times. Before it runs an action, it works out a fingerprint of everything that action could
depend on, and if it has run an action with the same fingerprint before, it puts the old outputs
back instead of running it again.

This chapter covers how that works and the commands built on it:

- **[The Action Key](ch05-01-action-key.md)**: what goes into the fingerprint, and how `buildl plan`
  tells you why a target has to run
- **[Early Cutoff](ch05-02-early-cutoff.md)**: why a target whose dependency was rebuilt to the same
  bytes is left alone
- **[Tests](ch05-03-tests.md)**: `buildl test`, and tests that are not run again when nothing they
  depend on changed
- **[`clean`, `gc` and `verify`](ch05-04-clean-gc-verify.md)**: removing outputs, trimming the store,
  and checking that what the cache holds is what the action really makes
