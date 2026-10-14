//! What stands at the path of a declared input, and the digest of its content.
//!
//! Its own directory because reading a workspace's files is one concern with one rule: an input
//! is a regular file inside the workspace. It is also the only module of the workspace that
//! hashes on a pool of threads, so that pool stays out of everything else.
//!
//! Responsibilities:
//!
//! - [`FileDigester`] — one look at a path that follows no symbolic link, and the SHA-256 of
//!   each file asked for, in the order asked.
//!
//! Non-responsibilities: keeping a digest from one build to the next, and naming the target
//! that declared an input that is missing or is not a file. Both belong to the pipeline.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod file_digester;

pub use file_digester::FileDigester;
