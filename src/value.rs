use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::{self, SerializeMap, SerializeSeq};
use serde::{Deserialize, Serialize};
use std::fmt;

/// RtonValue supports all RTON types, including specific integer widths and Binary Blobs.
#[derive(Debug, Clone, PartialEq)]
pub enum RtonValue {
    Null,
    Bool(bool),

    // Specific Integer Types
    Int8(i8),
    UInt8(u8),
    Int16(i16),
    UInt16(u16),
    Int32(i32),
    UInt32(u32),
    Int64(i64),
    UInt64(u64),

    Float(f32),
    Double(f64),
    String(String),
    Binary(Vec<u8>),
    Array(Vec<RtonValue>),
    Object(Vec<(String, RtonValue)>),
}

impl Serialize for RtonValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match self {
            RtonValue::Null => serializer.serialize_none(),
            RtonValue::Bool(b) => serializer.serialize_bool(*b),

            // Serialize specific integers
            RtonValue::Int8(v) => serializer.serialize_i8(*v),
            RtonValue::UInt8(v) => serializer.serialize_u8(*v),
            RtonValue::Int16(v) => serializer.serialize_i16(*v),
            RtonValue::UInt16(v) => serializer.serialize_u16(*v),
            RtonValue::Int32(v) => serializer.serialize_i32(*v),
            RtonValue::UInt32(v) => serializer.serialize_u32(*v),
            RtonValue::Int64(v) => serializer.serialize_i64(*v),
            RtonValue::UInt64(v) => serializer.serialize_u64(*v),

            RtonValue::Float(f) => serializer.serialize_f32(*f),
            RtonValue::Double(d) => serializer.serialize_f64(*d),
            RtonValue::String(s) => serializer.serialize_str(s),
            RtonValue::Binary(b) => serializer.serialize_bytes(b),
            RtonValue::Array(vec) => {
                let mut seq = serializer.serialize_seq(Some(vec.len()))?;
                for element in vec {
                    seq.serialize_element(element)?;
                }
                seq.end()
            }
            RtonValue::Object(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for RtonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct RtonValueVisitor;

        impl<'de> Visitor<'de> for RtonValueVisitor {
            type Value = RtonValue;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("any valid RTON value")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(RtonValue::Bool(value))
            }

            // Capture specific types from the Deserializer
            fn visit_i8<E>(self, value: i8) -> Result<Self::Value, E> {
                Ok(RtonValue::Int8(value))
            }
            fn visit_u8<E>(self, value: u8) -> Result<Self::Value, E> {
                Ok(RtonValue::UInt8(value))
            }
            fn visit_i16<E>(self, value: i16) -> Result<Self::Value, E> {
                Ok(RtonValue::Int16(value))
            }
            fn visit_u16<E>(self, value: u16) -> Result<Self::Value, E> {
                Ok(RtonValue::UInt16(value))
            }
            fn visit_i32<E>(self, value: i32) -> Result<Self::Value, E> {
                Ok(RtonValue::Int32(value))
            }
            fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E> {
                Ok(RtonValue::UInt32(value))
            }
            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(RtonValue::Int64(value))
            }
            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(RtonValue::UInt64(value))
            }

            fn visit_f32<E>(self, value: f32) -> Result<Self::Value, E> {
                Ok(RtonValue::Float(value))
            }
            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
                Ok(RtonValue::Double(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RtonValue::String(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RtonValue::String(value))
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RtonValue::Binary(v.to_vec()))
            }
            fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RtonValue::Binary(v))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(RtonValue::Null)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: de::Deserializer<'de>,
            {
                Deserialize::deserialize(deserializer)
            }

            fn visit_seq<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let mut vec = Vec::new();
                while let Some(elem) = visitor.next_element()? {
                    vec.push(elem);
                }
                Ok(RtonValue::Array(vec))
            }

            fn visit_map<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut entries = Vec::new();
                while let Some((key, value)) = visitor.next_entry()? {
                    entries.push((key, value));
                }
                Ok(RtonValue::Object(entries))
            }
        }

        deserializer.deserialize_any(RtonValueVisitor)
    }
}
