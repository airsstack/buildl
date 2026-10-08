//! The ceilings a build file's evaluation runs under.
//!
//! Its own file because the limits are the adapter's only configuration. The composition root is
//! meant to fill them from the workspace manifest's `[declaration]` table. They are this crate's
//! own type so that no caller has to name an airsl type to configure the adapter.
//!
//! Responsibilities: [`DeclarationLimits`] and its conversion into airsl's resource limits.
//!
//! Non-responsibilities: parsing the manifest, which happens before an adapter is built.

use core::num::NonZeroU64;

use airsl::{InstructionLimit, MemoryLimit, ResourceLimits};

/// 16 MiB, the declaration memory ceiling when the manifest sets none.
const DEFAULT_MEMORY_BYTES: NonZeroU64 = NonZeroU64::MIN.saturating_add(16 * 1024 * 1024 - 1);
/// The declaration instruction ceiling when the manifest sets none.
const DEFAULT_INSTRUCTIONS: NonZeroU64 = NonZeroU64::MIN.saturating_add(10_000_000 - 1);

/// 100 000 entries, the host-side source walk ceiling when the manifest sets none.
const DEFAULT_WALK_ENTRIES: NonZeroU64 = NonZeroU64::MIN.saturating_add(100_000 - 1);

/// The memory, instruction and source-walk ceilings for evaluating one build file.
///
/// The memory ceiling bounds the Lua heap, and it is also the byte budget for the declarations the
/// file stages outside that heap. The walk ceiling bounds the directory entries the host visits for
/// `buildl.sources`, counted across every call the file makes.
///
/// # Examples
///
/// ```
/// use std::num::NonZeroU64;
///
/// use buildl_lua::DeclarationLimits;
///
/// let defaults = DeclarationLimits::default();
/// assert_eq!(defaults.memory_bytes().get(), 16 * 1024 * 1024);
/// assert_eq!(defaults.instructions().get(), 10_000_000);
/// assert_eq!(defaults.walk_entries().get(), 100_000);
///
/// let tight = DeclarationLimits::new(NonZeroU64::MIN, NonZeroU64::MIN)
///     .with_walk_entries(NonZeroU64::MIN);
/// assert_eq!(tight.memory_bytes().get(), 1);
/// assert_eq!(tight.walk_entries().get(), 1);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclarationLimits {
    memory_bytes: NonZeroU64,
    instructions: NonZeroU64,
    walk_entries: NonZeroU64,
}

impl DeclarationLimits {
    /// Limits of `memory_bytes` bytes and `instructions` Lua VM instructions per build file, with
    /// the default walk ceiling.
    #[must_use]
    pub const fn new(memory_bytes: NonZeroU64, instructions: NonZeroU64) -> Self {
        Self {
            memory_bytes,
            instructions,
            walk_entries: DEFAULT_WALK_ENTRIES,
        }
    }

    /// The same limits with a ceiling of `walk_entries` directory entries per build file.
    #[must_use]
    pub const fn with_walk_entries(self, walk_entries: NonZeroU64) -> Self {
        Self {
            walk_entries,
            ..self
        }
    }

    /// The ceiling on directory entries the `buildl.sources` walk may visit, across all its calls
    /// in one build file.
    #[must_use]
    pub const fn walk_entries(&self) -> NonZeroU64 {
        self.walk_entries
    }

    /// The memory ceiling, and staging budget, in bytes.
    #[must_use]
    pub const fn memory_bytes(&self) -> NonZeroU64 {
        self.memory_bytes
    }

    /// The instruction ceiling.
    #[must_use]
    pub const fn instructions(&self) -> NonZeroU64 {
        self.instructions
    }

    /// The same ceilings in airsl's terms. A memory ceiling beyond the address space is clamped to
    /// it.
    pub(crate) fn resource_limits(&self) -> ResourceLimits {
        let memory = usize::try_from(self.memory_bytes.get()).unwrap_or(usize::MAX);
        ResourceLimits::new(
            Some(MemoryLimit::bytes(memory)),
            Some(InstructionLimit::count(self.instructions.get())),
        )
    }
}

impl Default for DeclarationLimits {
    /// 16 MiB, 10 000 000 instructions and 100 000 walk entries.
    fn default() -> Self {
        Self::new(DEFAULT_MEMORY_BYTES, DEFAULT_INSTRUCTIONS)
    }
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU64;

    use airsl::{InstructionLimit, MemoryLimit};

    use super::DeclarationLimits;

    #[test]
    fn defaults_to_sixteen_mebibytes_ten_million_instructions_and_a_hundred_thousand_entries() {
        let limits = DeclarationLimits::default();
        assert_eq!(limits.memory_bytes().get(), 16_777_216);
        assert_eq!(limits.instructions().get(), 10_000_000);
        assert_eq!(limits.walk_entries().get(), 100_000);
    }

    #[test]
    fn new_takes_the_default_walk_ceiling_and_with_walk_entries_replaces_only_it() {
        let two = NonZeroU64::MIN.saturating_add(1);
        let limits = DeclarationLimits::new(two, two);
        assert_eq!(limits.walk_entries().get(), 100_000);
        let walked = limits.with_walk_entries(NonZeroU64::MIN);
        assert_eq!(walked.walk_entries().get(), 1);
        assert_eq!(walked.memory_bytes(), two);
        assert_eq!(walked.instructions(), two);
    }

    #[test]
    fn converts_into_both_airsl_ceilings() {
        let limits = DeclarationLimits::new(NonZeroU64::MIN, NonZeroU64::MIN.saturating_add(1));
        let resource = limits.resource_limits();
        assert_eq!(resource.memory(), Some(MemoryLimit::bytes(1)));
        assert_eq!(resource.instructions(), Some(InstructionLimit::count(2)));
    }
}
