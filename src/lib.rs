pub mod binary; // Keep binary for BinaryBlob
// mod constants; // Moved to types
pub mod crypto;
pub mod de;
pub mod error;
// mod rtid; // Moved to types
pub mod ser;
pub mod types;
// mod value; // Moved to types
pub mod varint;

pub use binary::BinaryBlob; // Also re-exported from types usage?
pub use error::{Error, Result};
pub use types::{Rtid, RtidIdentifier, RtonIdentifier, RtonValue};
pub use varint::VarInt;

pub use de::{from_bytes, from_reader};
pub use ser::{to_bytes, to_writer};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FILE_HEADER;
    use serde::{Deserialize, Serialize};
    use std::io::Cursor;

    #[derive(Serialize)]
    struct VarIntU32(u32);

    #[derive(Serialize)]
    struct VarIntU64(u64);

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct SignedFields {
        small_i32: i32,
        negative_i32: i32,
        small_i64: i64,
        negative_i64: i64,
    }

    fn payload_tag(bytes: &[u8]) -> u8 {
        bytes[FILE_HEADER.len() + 4]
    }

    fn round_trip(value: &RtonValue) -> Result<RtonValue> {
        let bytes = to_bytes(value, None)?;
        from_bytes(&bytes, None)
    }

    #[test]
    fn test_rton_round_trip() {
        // Create a sample RtonValue
        let original = RtonValue::Object(vec![
            ("key1".to_string(), RtonValue::String("value1".to_string())),
            ("key2".to_string(), RtonValue::Int32(123)),
            (
                "key3".to_string(),
                RtonValue::Array(vec![RtonValue::Bool(true), RtonValue::Bool(false)]),
            ),
        ]);

        // Serialize to bytes (using default key/writer logic)
        let mut buffer = Vec::new();
        to_writer(&mut buffer, &original, None).expect("Serialization failed");

        // Deserialize from bytes
        let mut cursor = Cursor::new(buffer);
        let decoded: RtonValue = from_reader(&mut cursor, None).expect("Deserialization failed");

        // Verify equality
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_rtid_uid_round_trip() {
        let original = RtonValue::Object(vec![(
            "a".to_string(),
            RtonValue::Rtid(Rtid::Uid {
                group: 0x2,
                id: 0x1,
                obj: 0x00000003,
                name: Some("sheet".to_string()),
            }),
        )]);

        let decoded = round_trip(&original).expect("RTID UID round-trip failed");
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_rtid_raw_round_trip() {
        let original = RtonValue::Object(vec![(
            "a".to_string(),
            RtonValue::Rtid(Rtid::Raw {
                name: "name".to_string(),
                parent: "parent".to_string(),
            }),
        )]);

        let decoded = round_trip(&original).expect("RTID raw round-trip failed");
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_missing_done_footer_is_rejected() {
        let original = RtonValue::Object(vec![("a".to_string(), RtonValue::Int32(1))]);
        let mut bytes = to_bytes(&original, None).expect("Serialization failed");
        bytes.truncate(bytes.len() - 4);

        let error = from_bytes::<RtonValue>(&bytes, None).expect_err("Expected footer error");
        assert!(matches!(error, Error::InvalidFooter));
    }

    #[test]
    fn test_array_capacity_must_match_declared_length() {
        let original = RtonValue::Object(vec![(
            "a".to_string(),
            RtonValue::Array(vec![RtonValue::UInt8(1), RtonValue::UInt8(2)]),
        )]);
        let mut bytes = to_bytes(&original, None).expect("Serialization failed");

        let capacity_idx = bytes
            .iter()
            .position(|&byte| byte == RtonIdentifier::ArrayCapacity as u8)
            .expect("Array capacity tag missing")
            + 1;
        bytes[capacity_idx] = 3;

        let error = from_bytes::<RtonValue>(&bytes, None).expect_err("Expected array mismatch");
        assert!(matches!(error, Error::ArrayLengthMismatch));
    }

    #[test]
    fn test_explicit_u32_varint_uses_alt_tag() {
        let bytes = to_bytes(&VarIntU32(300), None).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonIdentifier::VarIntU32Alt as u8);
    }

    #[test]
    fn test_explicit_u64_varint_uses_alt_tag() {
        let bytes = to_bytes(&VarIntU64(300), None).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonIdentifier::VarIntU64Alt as u8);
    }

    #[test]
    fn test_small_positive_i32_uses_compact_signed_positive_tag() {
        let bytes = to_bytes(&123i32, None).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonIdentifier::VarIntU32 as u8);
    }

    #[test]
    fn test_negative_i32_uses_zigzag_tag() {
        let bytes = to_bytes(&-1i32, None).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonIdentifier::VarIntI32 as u8);
    }

    #[test]
    fn test_large_i32_uses_fixed_width_tag() {
        let bytes = to_bytes(&(1i32 << 21), None).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonIdentifier::Int32 as u8);
    }

    #[test]
    fn test_small_positive_i64_uses_compact_signed_positive_tag() {
        let bytes = to_bytes(&123i64, None).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonIdentifier::VarIntU64 as u8);
    }

    #[test]
    fn test_negative_i64_uses_zigzag_tag() {
        let bytes = to_bytes(&-1i64, None).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonIdentifier::VarIntI64 as u8);
    }

    #[test]
    fn test_large_i64_uses_fixed_width_tag() {
        let bytes = to_bytes(&(1i64 << 49), None).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonIdentifier::Int64 as u8);
    }

    #[test]
    fn test_signed_fields_round_trip_with_compact_tags() {
        let original = SignedFields {
            small_i32: 123,
            negative_i32: -1,
            small_i64: 123,
            negative_i64: -1,
        };

        let decoded = from_bytes::<SignedFields>(&to_bytes(&original, None).expect("Serialization failed"), None)
            .expect("Deserialization failed");

        assert_eq!(decoded, original);
    }
}
