//! The `.buildlignore` files of a workspace, behind the port that asks whether a path is ignored.
//!
//! Its own module because reading ignore files and matching their patterns is filesystem work
//! the core never does, and the `ignore` crate that matches them is named here and nowhere else.
//!
//! Responsibilities: [`BuildlIgnore`], the implementation of `buildl_core::IgnoreRules`.
//!
//! The files are written as `.gitignore` is, and a path is decided as git decides it:
//!
//! - A `.buildlignore` sits in any folder of the workspace and applies to that folder and
//!   everything below it. Only files at or below the workspace root are read, and no other ignore
//!   file is: not `.gitignore`, not `.ignore`.
//! - A line starting with `#` is a comment. `!` re-includes, a leading `/` anchors a pattern to
//!   its folder, a trailing `/` matches folders only, a pattern with no `/` matches at any depth,
//!   and matching is case-sensitive. A leading byte-order mark is dropped, and `{` and `}` are
//!   literal, never alternation.
//! - The folders on the way to a path are asked first, from the root down, and then the path. The
//!   first to be ignored decides, so a `!` line cannot re-include anything under an ignored folder.
//!   For a folder or path, the files of the root and of every folder above it are asked, deepest
//!   first, and within a file the last matching line wins.
//! - A file that cannot be used, because a pattern does not parse, its bytes are not UTF-8, it is
//!   a symlink or it cannot be read, is refused rather than skipped.
//!
//! Non-responsibilities: what an ignored path means to a declaration, and the walk of the
//! workspace, which ask through the port.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to
//! unit-test.

mod buildl_ignore;
mod folder_rules;

pub use buildl_ignore::BuildlIgnore;
