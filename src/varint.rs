use serde::{Deserialize, Serialize, Serializer};

// ============================================================================
// VarInt — force compact varint encoding for integers
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VarInt<T>(pub T);

impl Serialize for VarInt<i32> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_newtype_struct("VarIntI32", &self.0)
    }
}

impl Serialize for VarInt<u32> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_newtype_struct("VarIntU32", &self.0)
    }
}

impl Serialize for VarInt<i64> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_newtype_struct("VarIntI64", &self.0)
    }
}

impl Serialize for VarInt<u64> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_newtype_struct("VarIntU64", &self.0)
    }
}

// ============================================================================
// DirectStr — force direct string encoding (0x81/0x82) without interning
//
// PvZ2 uses direct strings when `arg3 == 0` in the ASCII/UTF-8 writer
// helpers (`sub_1024e76bc` / `sub_1024e77cc`).  Wrap a `&str` or `String`
// in `DirectStr` to skip the interning cache and emit tag 0x81 or 0x82.
// ============================================================================

/// Wrapper that forces direct string encoding (tags 0x81/0x82).
///
/// By default the RTON serializer interns all strings (tags 0x90–0x93).
/// Wrap with `DirectStr` to emit a direct string instead — matching PvZ2's
/// `arg3 == 0` code path.
///
/// # Example
///
/// ```ignore
/// use rton::DirectStr;
///
/// #[derive(Serialize)]
/// struct MyData {
///     cached_name: String,           // emitted as 0x90/0x92 (interned)
///     direct_tag: DirectStr<String>, // emitted as 0x81/0x82 (direct)
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DirectStr<T: AsRef<str>>(pub T);

impl<T: AsRef<str> + Serialize> Serialize for DirectStr<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_newtype_struct("DirectStr", &self.0.as_ref())
    }
}

impl<'de, T: AsRef<str> + Deserialize<'de>> Deserialize<'de> for DirectStr<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        T::deserialize(deserializer).map(DirectStr)
    }
}
