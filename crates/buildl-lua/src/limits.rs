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

/// The memory and instruction ceilings for evaluating one build file.
///
/// The memory ceiling bounds the Lua heap, and it is also the byte budget for the declarations the
/// file stages outside that heap.
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
///
/// let tight = DeclarationLimits::new(NonZeroU64::MIN, NonZeroU64::MIN);
/// assert_eq!(tight.memory_bytes().get(), 1);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclarationLimits {
    memory_bytes: NonZeroU64,
    instructions: NonZeroU64,
}

impl DeclarationLimits {
    /// Limits of `memory_bytes` bytes and `instructions` Lua VM instructions per build file.
    #[must_use]
    pub const fn new(memory_bytes: NonZeroU64, instructions: NonZeroU64) -> Self {
        Self {
            memory_bytes,
            instructions,
        }
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
    /// 16 MiB and 10 000 000 instructions.
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
    fn defaults_to_sixteen_mebibytes_and_ten_million_instructions() {
        let limits = DeclarationLimits::default();
        assert_eq!(limits.memory_bytes().get(), 16_777_216);
        assert_eq!(limits.instructions().get(), 10_000_000);
    }

    #[test]
    fn converts_into_both_airsl_ceilings() {
        let limits = DeclarationLimits::new(NonZeroU64::MIN, NonZeroU64::MIN.saturating_add(1));
        let resource = limits.resource_limits();
        assert_eq!(resource.memory(), Some(MemoryLimit::bytes(1)));
        assert_eq!(resource.instructions(), Some(InstructionLimit::count(2)));
    }
}
