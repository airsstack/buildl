# Plugins

Write the same three targets for every Go program in a repository and you soon want a function
for it. In buildl that function lives in a **plugin**: Lua that runs while build files are read,
and declares targets for you. A plugin cannot run anything. All it can do is call the same
`buildl.target` a build file calls, so whatever it declares is planned, held to the ceiling,
cached and shown by `buildl graph` like a target you wrote by hand.

A plugin is not trusted just because it is code you loaded. Every target it declares carries its
name, and is held to what the plugin said it needs before it is held to the ceiling.

This chapter covers:

- **[Writing a Plugin](ch08-01-writing-a-plugin.md)**: `extension.toml`, `buildl.use`, and where
  your own logic should live
- **[Plugins from Git](ch08-02-plugins-from-git.md)**: a plugin kept in another repository,
  `buildl plugins fetch`, and `buildl.lock`
- **[How buildl Decides](ch08-03-how-buildl-decides.md)**: what a plugin's targets may want, and
  how `plan`, `doctor`, the approver and `graph` tell you which plugin wanted it
