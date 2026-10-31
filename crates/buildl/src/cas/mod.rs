//! The content store on disk: bytes held by their digest, and held bytes made files of the
//! workspace.
//!
//! Its own module because it is the one place that decides how content is laid out under the
//! state directory and how it reaches the output directory: a read-only blob per digest, beside
//! it an executable twin for content that was placed as a file that may be run, a hard link from
//! the output directory for a plain file wherever a write through one is seen to be refused, and
//! a copy of the twin for a file that may be run.
//!
//! Responsibilities: [`LocalCas`], the [`ContentStore`](buildl_core::ContentStore) of a local
//! workspace.
//!
//! Non-responsibilities: deciding what is stored, cleared or placed, which the build does; and
//! where the state directory keeps the store, which
//! [`StateLayout`](crate::state::StateLayout) says.
//!
//! This file holds only module declarations and re-exports, so it carries no logic to unit-test.

mod blob;
mod local_cas;
mod placement;

pub use local_cas::LocalCas;
