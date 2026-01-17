use crate::binary::BinaryBlob;
use crate::rtid::Rtid;
use crate::varint::VarInt;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::{self, SerializeMap, SerializeSeq};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
pub enum RtonValue {
    Null,
    Bool(bool),
    Int8(i8),
    UInt8(u8),
    Int16(i16),
    UInt16(u16),
    Int32(i32),
    UInt32(u32),
    Int64(i64),
    UInt64(u64),
    VarIntI32(VarInt<i32>),
    VarIntU32(VarInt<u32>),
    VarIntI64(VarInt<i64>),
    VarIntU64(VarInt<u64>),
    Float(f32),
    Double(f64),
    String(String),
    Binary(BinaryBlob),
    Rtid(Rtid),
    Array(Vec<RtonValue>),
    Object(Vec<(String, RtonValue)>),
}

impl RtonValue {
    pub fn new_int(v: i64) -> Self {
        if (i8::MIN as i64..=i8::MAX as i64).contains(&v) {
            RtonValue::Int8(v as i8)
        } else if (i16::MIN as i64..=i16::MAX as i64).contains(&v) {
            RtonValue::Int16(v as i16)
        } else if (i32::MIN as i64..=i32::MAX as i64).contains(&v) {
            RtonValue::Int32(v as i32)
        } else {
            RtonValue::Int64(v)
        }
    }
    pub fn new_uint(v: u64) -> Self {
        if v <= u8::MAX as u64 {
            RtonValue::UInt8(v as u8)
        } else if v <= u16::MAX as u64 {
            RtonValue::UInt16(v as u16)
        } else if v <= u32::MAX as u64 {
            RtonValue::UInt32(v as u32)
        } else {
            RtonValue::UInt64(v)
        }
    }
}

impl Serialize for RtonValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match self {
            RtonValue::Null => serializer.serialize_none(),
            RtonValue::Bool(b) => serializer.serialize_bool(*b),
            RtonValue::Int8(v) => serializer.serialize_i8(*v),
            RtonValue::UInt8(v) => serializer.serialize_u8(*v),
            RtonValue::Int16(v) => serializer.serialize_i16(*v),
            RtonValue::UInt16(v) => serializer.serialize_u16(*v),
            RtonValue::Int32(v) => serializer.serialize_i32(*v),
            RtonValue::UInt32(v) => serializer.serialize_u32(*v),
            RtonValue::Int64(v) => serializer.serialize_i64(*v),
            RtonValue::UInt64(v) => serializer.serialize_u64(*v),
            RtonValue::VarIntI32(v) => {
                if serializer.is_human_readable() {
                    serializer.serialize_i32(v.0)
                } else {
                    v.serialize(serializer)
                }
            }
            RtonValue::VarIntU32(v) => {
                if serializer.is_human_readable() {
                    serializer.serialize_u32(v.0)
                } else {
                    v.serialize(serializer)
                }
            }
            RtonValue::VarIntI64(v) => {
                if serializer.is_human_readable() {
                    serializer.serialize_i64(v.0)
                } else {
                    v.serialize(serializer)
                }
            }
            RtonValue::VarIntU64(v) => {
                if serializer.is_human_readable() {
                    serializer.serialize_u64(v.0)
                } else {
                    v.serialize(serializer)
                }
            }
            RtonValue::Float(f) => {
                if serializer.is_human_readable() && !f.is_finite() {
                    if f.is_nan() {
                        serializer.serialize_str("NaN")
                    } else if *f == f32::INFINITY {
                        serializer.serialize_str("Infinity")
                    } else {
                        serializer.serialize_str("-Infinity")
                    }
                } else {
                    serializer.serialize_f32(*f)
                }
            }
            RtonValue::Double(d) => {
                if serializer.is_human_readable() && !d.is_finite() {
                    if d.is_nan() {
                        serializer.serialize_str("NaN")
                    } else if *d == f64::INFINITY {
                        serializer.serialize_str("Infinity")
                    } else {
                        serializer.serialize_str("-Infinity")
                    }
                } else {
                    serializer.serialize_f64(*d)
                }
            }
            RtonValue::String(s) => serializer.serialize_str(s),
            RtonValue::Binary(b) => {
                if serializer.is_human_readable() {
                    serializer.serialize_str(&b.to_string())
                } else {
                    b.serialize(serializer)
                }
            }
            RtonValue::Rtid(rtid) => {
                if serializer.is_human_readable() {
                    serializer.serialize_str(&rtid.to_string())
                } else {
                    match rtid {
                        Rtid::Null => rtid.serialize(serializer),
                        _ => serializer.serialize_newtype_struct("RTID", rtid),
                    }
                }
            }
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
                Ok(RtonValue::new_int(value))
            }
            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(RtonValue::new_uint(value))
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
                match value {
                    "NaN" => return Ok(RtonValue::Double(f64::NAN)),
                    "Infinity" | "+Infinity" => return Ok(RtonValue::Double(f64::INFINITY)),
                    "-Infinity" => return Ok(RtonValue::Double(f64::NEG_INFINITY)),
                    _ => {}
                }
                if value.starts_with("$BINARY(\"")
                    && let Ok(blob) = BinaryBlob::from_str(value)
                {
                    return Ok(RtonValue::Binary(blob));
                }
                if value.starts_with("RTID(")
                    && let Ok(rtid) = Rtid::from_str(value)
                {
                    return Ok(RtonValue::Rtid(rtid));
                }
                Ok(RtonValue::String(value.to_owned()))
            }
            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }
            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RtonValue::Binary(BinaryBlob(v.to_vec())))
            }
            fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RtonValue::Binary(BinaryBlob(v)))
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
