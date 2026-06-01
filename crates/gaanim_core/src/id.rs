/// A zero-dependency, unique identifier for Mobjects (graphical objects) in `gaanim`.
///
/// Under the hood, this is a 64-bit integer that aligns perfectly with Bevy's `Entity` representation
/// (using the lower 32-bits for the index and the upper 32-bits for the generation).
/// This allows `gaanim_core` to remain completely independent of the Bevy monolith,
/// enabling extremely fast compile times and potential WASM/standalone usage, while retaining
/// seamless compatibility with the Bevy ECS backend in other crates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ObjectId(u64);

impl ObjectId {
    /// Creates a new `ObjectId` from a raw 64-bit integer.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the raw 64-bit integer representation.
    pub const fn as_raw(&self) -> u64 {
        self.0
    }

    /// Extracts the 32-bit index of the identifier.
    /// In Bevy, this corresponds to the entity index.
    pub const fn index(&self) -> u32 {
        self.0 as u32
    }

    /// Extracts the 32-bit generation of the identifier.
    /// In Bevy, this corresponds to the entity generation.
    pub const fn generation(&self) -> u32 {
        (self.0 >> 32) as u32
    }

    /// Creates a new `ObjectId` from index and generation parts.
    pub const fn from_parts(index: u32, generation: u32) -> Self {
        Self(((generation as u64) << 32) | (index as u64))
    }
}

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ObjectId({}v{})", self.index(), self.generation())
    }
}

// Convert from/into u64 for serialization and raw FFI/Python interfacing
impl From<u64> for ObjectId {
    fn from(raw: u64) -> Self {
        Self(raw)
    }
}

impl From<ObjectId> for u64 {
    fn from(id: ObjectId) -> Self {
        id.0
    }
}
