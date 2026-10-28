# Asking Permission

A build runs programs on your machine. Most build tools let a build file run whatever it likes, and
you find out what it did afterwards. buildl works the other way round: before anything runs, it
works out everything the build will need, holds that against a list you wrote, and stops if
anything is missing from the list.

This chapter covers:

- [Nothing Is Ambient](ch04-01-nothing-is-ambient.md): how buildl works out what a build needs,
  and why a build file never asks for permission itself.
- [`buildl.toml`, the Ceiling](ch04-02-the-ceiling.md): the file that says the most any build in
  the workspace may do.
- [Approvers](ch04-03-approvers.md): who says yes once a build fits under the ceiling, and
  `buildl doctor`, which shows you the whole picture.
