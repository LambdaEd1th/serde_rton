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
pub use varint::{DirectStr, VarInt};

pub use de::{from_bytes, from_reader};
pub use ser::{to_bytes, to_writer};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FILE_HEADER;
    use serde::{Deserialize, Serialize};
    use std::io::Cursor;
    use std::str::FromStr;

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

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct UnsignedFields {
        small_u32: u32,
        large_u32: u32,
        small_u64: u64,
        large_u64: u64,
    }

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct DirectStringFields {
        #[serde(rename = "cached")]
        cached_name: String,
        #[serde(rename = "direct")]
        direct_tag: DirectStr<String>,
    }

    fn payload_tag(bytes: &[u8]) -> u8 {
        bytes[FILE_HEADER.len() + 4]
    }

    fn round_trip(value: &RtonValue) -> Result<RtonValue> {
        let bytes = to_bytes(value)?;
        from_bytes(&bytes)
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
        to_writer(&mut buffer, &original).expect("Serialization failed");

        // Deserialize from bytes
        let mut cursor = Cursor::new(buffer);
        let decoded: RtonValue = from_reader(&mut cursor).expect("Deserialization failed");

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
    fn test_rtid_colon_format_parsing() {
        // Hopper: PvZ2 RtIdProtocol emits colon-prefixed RTID strings
        // in runtime protocol messages.  Verify we can parse all variants.

        // :%d.%d — decimal id.group only (obj=0, no name)
        let r = Rtid::from_str("RTID(:123.456)").expect("colon short parse");
        assert_eq!(
            r,
            Rtid::Uid {
                id: 123,
                group: 456,
                obj: 0,
                name: None,
            }
        );

        // :%d.%d@%d — decimal id.group@obj
        let r = Rtid::from_str("RTID(:789.012@345)").expect("colon no-name parse");
        assert_eq!(
            r,
            Rtid::Uid {
                id: 789,
                group: 12,
                obj: 345,
                name: None,
            }
        );

        // :%d.%d.%08x@%s — decimal id, decimal group, hex obj, string name
        let r = Rtid::from_str("RTID(:1.2.0000000a@SomeName)").expect("colon full parse");
        assert_eq!(
            r,
            Rtid::Uid {
                id: 1,
                group: 2,
                obj: 0xa,
                name: Some("SomeName".to_string()),
            }
        );

        // Existing hex format still works
        let r = Rtid::from_str("RTID(1a.2b.0000000c@name)").expect("hex parse");
        assert_eq!(
            r,
            Rtid::Uid {
                id: 0x1a,
                group: 0x2b,
                obj: 0xc,
                name: Some("name".to_string()),
            }
        );
    }

    #[test]
    fn test_missing_done_footer_is_rejected() {
        let original = RtonValue::Object(vec![("a".to_string(), RtonValue::Int32(1))]);
        let mut bytes = to_bytes(&original).expect("Serialization failed");
        bytes.truncate(bytes.len() - 4);

        let error = from_bytes::<RtonValue>(&bytes).expect_err("Expected footer error");
        assert!(matches!(error, Error::InvalidFooter));
    }

    #[test]
    fn test_array_capacity_must_match_declared_length() {
        let original = RtonValue::Object(vec![(
            "a".to_string(),
            RtonValue::Array(vec![RtonValue::UInt8(1), RtonValue::UInt8(2)]),
        )]);
        let mut bytes = to_bytes(&original).expect("Serialization failed");

        let capacity_idx = bytes
            .iter()
            .position(|&byte| byte == RtonIdentifier::ArrayCapacity as u8)
            .expect("Array capacity tag missing")
            + 1;
        bytes[capacity_idx] = 3;

        let error = from_bytes::<RtonValue>(&bytes).expect_err("Expected array mismatch");
        assert!(matches!(error, Error::ArrayLengthMismatch));
    }

    #[test]
    fn test_explicit_u32_varint_uses_alt_tag() {
        let bytes = to_bytes(&VarIntU32(300)).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonIdentifier::VarIntU32Alt as u8);
    }

    #[test]
    fn test_explicit_u64_varint_uses_alt_tag() {
        let bytes = to_bytes(&VarIntU64(300)).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonIdentifier::VarIntU64Alt as u8);
    }

    #[test]
    fn test_small_positive_i32_uses_compact_signed_positive_tag() {
        let bytes = to_bytes(&123i32).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonIdentifier::VarIntU32 as u8);
    }

    #[test]
    fn test_negative_i32_uses_zigzag_tag() {
        let bytes = to_bytes(&-1i32).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonIdentifier::VarIntI32 as u8);
    }

    #[test]
    fn test_large_i32_uses_fixed_width_tag() {
        let bytes = to_bytes(&(1i32 << 21)).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonIdentifier::Int32 as u8);
    }

    #[test]
    fn test_small_positive_i64_uses_compact_signed_positive_tag() {
        let bytes = to_bytes(&123i64).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonIdentifier::VarIntU64 as u8);
    }

    #[test]
    fn test_negative_i64_uses_zigzag_tag() {
        let bytes = to_bytes(&-1i64).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonIdentifier::VarIntI64 as u8);
    }

    #[test]
    fn test_large_i64_uses_fixed_width_tag() {
        let bytes = to_bytes(&(1i64 << 49)).expect("Serialization failed");

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

        let decoded =
            from_bytes::<SignedFields>(&to_bytes(&original).expect("Serialization failed"))
                .expect("Deserialization failed");

        assert_eq!(decoded, original);
    }

    // ——— Adaptive unsigned integer encoding ———

    #[test]
    fn test_small_u32_uses_compact_alt_tag() {
        // 300 fits in 2 varint bytes (< 4) → should use 0x28 (VarIntU32Alt)
        let bytes = to_bytes(&300u32).expect("Serialization failed");
        assert_eq!(payload_tag(&bytes), RtonIdentifier::VarIntU32Alt as u8);
    }

    #[test]
    fn test_large_u32_uses_fixed_width_tag() {
        // 1 << 28 needs 5 varint bytes (>= 4) → should use 0x26 (UInt32)
        let bytes = to_bytes(&(1u32 << 28)).expect("Serialization failed");
        assert_eq!(payload_tag(&bytes), RtonIdentifier::UInt32 as u8);
    }

    #[test]
    fn test_small_u64_uses_compact_alt_tag() {
        // 300 fits in 2 varint bytes (< 8) → should use 0x48 (VarIntU64Alt)
        let bytes = to_bytes(&300u64).expect("Serialization failed");
        assert_eq!(payload_tag(&bytes), RtonIdentifier::VarIntU64Alt as u8);
    }

    #[test]
    fn test_large_u64_uses_fixed_width_tag() {
        // 1 << 56 needs 9 varint bytes (>= 8) → should use 0x46 (UInt64)
        let bytes = to_bytes(&(1u64 << 56)).expect("Serialization failed");
        assert_eq!(payload_tag(&bytes), RtonIdentifier::UInt64 as u8);
    }

    #[test]
    fn test_unsigned_fields_round_trip_with_compact_tags() {
        let original = UnsignedFields {
            small_u32: 300,
            large_u32: 1 << 28,
            small_u64: 300,
            large_u64: 1u64 << 56,
        };

        let decoded =
            from_bytes::<UnsignedFields>(&to_bytes(&original).expect("Serialization failed"))
                .expect("Deserialization failed");

        assert_eq!(decoded, original);
    }

    // ——— DirectStr ———

    #[test]
    fn test_direct_str_ascii_uses_direct_tag() {
        let val = DirectStringFields {
            cached_name: "hello".to_string(),
            direct_tag: DirectStr("world".to_string()),
        };
        let bytes = to_bytes(&val).expect("Serialization failed");

        // The file structure is: RTON header (8B) → object start → key+value pairs
        // We just need to verify the bytes contain both 0x90 (cached) and 0x81 (direct)
        let body = &bytes[FILE_HEADER.len() + 4..];
        assert!(
            body.windows(1)
                .any(|w| w[0] == RtonIdentifier::StrAsciiDef as u8),
            "cached string should use 0x90 (StrAsciiDef)"
        );
        assert!(
            body.windows(1)
                .any(|w| w[0] == RtonIdentifier::StrAsciiDirect as u8),
            "direct string should use 0x81 (StrAsciiDirect)"
        );
    }
}
