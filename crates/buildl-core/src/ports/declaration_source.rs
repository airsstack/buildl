//! The port through which Load evaluates one build file.
//!
//! Its own file because evaluating a build file is the one thing Load cannot do itself: it means
//! reading a file and running a language this crate does not know. Everything around that one call
//! — which files to evaluate, in what order, and what their results mean — stays in Load.
//!
//! Responsibilities: [`DeclarationSource`].
//!
//! Non-responsibilities: implementing it. The real implementation belongs in an adapter crate.
//!
//! This file holds only a trait definition, so it carries no logic to unit-test.

use crate::error::Result;
use crate::types::{BuildFile, Evaluated};

/// Evaluates one build file and returns what it staged, exactly as written.
///
/// An implementation reports a missing file as [`Evaluated::Absent`] rather than as an error:
/// what a missing file means depends on who asked for it, which only the caller knows.
///
/// # Examples
///
/// An implementation needs nothing but this crate's public API:
///
/// ```
/// use std::path::PathBuf;
///
/// use buildl_core::{
///     BuildFile, DeclarationOrder, DeclarationSource, Directory, Evaluated, Provenance, Result,
///     StagedFile, StagedSubdir, Written,
/// };
///
/// /// A source in which only the workspace root has a build file, requesting `lib`.
/// struct RootOnly;
///
/// impl DeclarationSource for RootOnly {
///     fn evaluate(&self, file: &BuildFile) -> Result<Evaluated> {
///         if !file.directory().is_root() {
///             return Ok(Evaluated::Absent);
///         }
///         Ok(Evaluated::Staged(StagedFile {
///             declarations: Vec::new(),
///             subdirs: vec![StagedSubdir {
///                 path: Written::new("lib"),
///                 order: DeclarationOrder::new(0),
///             }],
///         }))
///     }
/// }
///
/// let lib = BuildFile::new(Provenance::new(
///     PathBuf::from("lib/build.lua"),
///     Directory::parse("lib")?,
/// ));
/// assert_eq!(RootOnly.evaluate(&lib)?, Evaluated::Absent);
/// # Ok::<(), buildl_core::Error>(())
/// ```
pub trait DeclarationSource {
    /// Evaluates `file`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Evaluation` when the file exists but cannot be evaluated.
    fn evaluate(&self, file: &BuildFile) -> Result<Evaluated>;
}
