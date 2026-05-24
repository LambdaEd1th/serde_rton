use crate::binary::BinaryBlob;
use crate::error::Error;
use crate::varint::VarInt;
use num_enum::TryFromPrimitive;
use regex::Regex;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::{self, SerializeMap, SerializeSeq, SerializeTuple};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use std::sync::OnceLock;

// ================= CONSTANTS =================

pub const FILE_HEADER: &[u8] = b"RTON";
pub const FILE_FOOTER: &[u8] = b"DONE";
pub const FILE_VERSION: u32 = 1;

#[derive(Debug, Eq, PartialEq, TryFromPrimitive, Clone, Copy)]
#[repr(u8)]
pub enum RtonIdentifier {
    BoolFalse = 0x00,
    BoolTrue = 0x01,
    StrNull = 0x02,

    Int8 = 0x08,
    Int8Zero = 0x09,
    UInt8 = 0x0a,
    UIntZero = 0x0b,

    Int16 = 0x10,
    Int16Zero = 0x11,
    UInt16 = 0x12,
    UInt16Zero = 0x13,

    Int32 = 0x20,
    Int32Zero = 0x21,
    UInt32 = 0x26,
    UInt32Zero = 0x27,

    Int64 = 0x40,
    Int64Zero = 0x41,
    UInt64 = 0x46,
    UInt64Zero = 0x47,

    VarIntU32 = 0x24,
    VarIntI32 = 0x25,
    VarIntU32Alt = 0x28,
    VarIntI32Alt = 0x29,

    VarIntU64 = 0x44,
    VarIntI64 = 0x45,
    VarIntU64Alt = 0x48,
    VarIntI64Alt = 0x49,

    Float = 0x22,
    FloatZero = 0x23,
    Double = 0x42,
    DoubleZero = 0x43,

    StrAsciiDirect = 0x81,
    StrUtf8Direct = 0x82,
    StrAsciiDef = 0x90,
    StrAsciiRef = 0x91,
    StrUtf8Def = 0x92,
    StrUtf8Ref = 0x93,

    BinaryBlob = 0x87,

    Rtid = 0x83,
    RtidZero = 0x84,

    ObjectStart = 0x85,
    ArrayStart = 0x86,

    ArrayCapacity = 0xfd,

    ArrayEnd = 0xfe,

    ObjectEnd = 0xff,

    StrNativeX1 = 0xB0,
    StrNativeX2 = 0xB1,
    StrUnicodeX1 = 0xB2,
    StrUnicodeX2 = 0xB3,
    StrNativeOrUnicodeX1 = 0xB4,
    StrNativeOrUnicodeX2 = 0xB5,
    StrNativeOrUnicodeX3 = 0xB6,
    StrNativeOrUnicodeX4 = 0xB7,
    ObjectStartX1 = 0xB8,
    ArrayStartX1 = 0xB9,
    StrNativeX3 = 0xBA,
    StrBinaryBlobX1 = 0xBB,
    BoolX1 = 0xBC,
}

#[derive(Debug, Eq, PartialEq, TryFromPrimitive, Clone, Copy)]
#[repr(u8)]
pub enum RtidIdentifier {
    Zero = 0x00,
    UidNoString = 0x01,
    Uid = 0x02,
    String = 0x03,
}

// ================= RTID =================

#[derive(Debug, Clone, PartialEq)]
pub enum Rtid {
    Null,
    /// Format: group.id.obj@name
    Uid {
        group: u64,
        id: u64,
        obj: u32,
        name: Option<String>,
    },
    /// Format: name@parent
    Raw {
        name: String,
        parent: String,
    },
}

impl fmt::Display for Rtid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rtid::Null => write!(f, "RTID(0)"),
            Rtid::Uid {
                group,
                id,
                obj,
                name,
            } => {
                if let Some(n) = name {
                    write!(f, "RTID({:x}.{:x}.{:08x}@{})", id, group, obj, n)
                } else {
                    write!(f, "RTID({:x}.{:x}.{:08x}@)", id, group, obj)
                }
            }
            Rtid::Raw { name, parent } => write!(f, "RTID({}@{})", name, parent),
        }
    }
}

impl FromStr for Rtid {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Step 1: Match Outer Wrapper "RTID(...)"
        static OUTER_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
        let outer_re = OUTER_REGEX
            .get_or_init(|| Regex::new(r"^RTID\((.*)\)$"))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        let caps = outer_re
            .captures(s)
            .ok_or_else(|| Error::InvalidRtid("Not an RTID string".into()))?;
        let inner = caps
            .get(1)
            .ok_or_else(|| Error::InvalidRtid("Empty content".into()))?
            .as_str();

        // Step 2: Analyze Content
        if inner == "0" {
            return Ok(Rtid::Null);
        }

        // Case B: UID (Strict Lowercase Hex)
        static UID_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
        let uid_re = UID_REGEX
            .get_or_init(|| Regex::new(r"^([0-9a-f]+)\.([0-9a-f]+)\.([0-9a-f]+)@(.*)$"))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        if let Some(caps) = uid_re.captures(inner) {
            let id_str = caps.get(1).unwrap().as_str();
            let group_str = caps.get(2).unwrap().as_str();
            let obj_str = caps.get(3).unwrap().as_str();
            let name_str = caps.get(4).unwrap().as_str();

            let id = u64::from_str_radix(id_str, 16)?;
            let group = u64::from_str_radix(group_str, 16)?;
            let obj = u32::from_str_radix(obj_str, 16)?;

            let name = if name_str.is_empty() {
                None
            } else {
                Some(name_str.to_string())
            };

            return Ok(Rtid::Uid {
                group,
                id,
                obj,
                name,
            });
        }

        // Case C: Raw
        static RAW_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
        let raw_re = RAW_REGEX
            .get_or_init(|| Regex::new(r"^([^@]+)@([^@]*)$"))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        if let Some(caps) = raw_re.captures(inner) {
            let name = caps.get(1).unwrap().as_str();
            let parent = caps.get(2).unwrap().as_str();
            return Ok(Rtid::Raw {
                name: name.to_string(),
                parent: parent.to_string(),
            });
        }

        Err(Error::InvalidRtid("Inner structure mismatch".into()))
    }
}

impl Serialize for Rtid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match self {
            Rtid::Null => serializer.serialize_unit_variant("RTID", 0x84, "Zero"),
            _ => serializer.serialize_newtype_struct("RTID", &RtidBody(self)),
        }
    }
}

impl<'de> Deserialize<'de> for Rtid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct RtidVisitor;

        impl<'de> Visitor<'de> for RtidVisitor {
            type Value = Rtid;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an RTID string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Rtid::from_str(value).map_err(E::custom)
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }
        }

        deserializer.deserialize_string(RtidVisitor)
    }
}

struct RtidBody<'a>(&'a Rtid);

impl Serialize for RtidBody<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match self.0 {
            Rtid::Null => serializer.serialize_unit_variant("RTID", 0x84, "Zero"),
            Rtid::Uid {
                group,
                id,
                obj,
                name,
            } => {
                if let Some(name) = name {
                    let mut tup = serializer.serialize_tuple(5)?;
                    tup.serialize_element(&OverrideByte(2))?;
                    tup.serialize_element(name)?;
                    tup.serialize_element(group)?;
                    tup.serialize_element(id)?;
                    tup.serialize_element(obj)?;
                    tup.end()
                } else {
                    let mut tup = serializer.serialize_tuple(4)?;
                    tup.serialize_element(&OverrideByte(1))?;
                    tup.serialize_element(group)?;
                    tup.serialize_element(id)?;
                    tup.serialize_element(obj)?;
                    tup.end()
                }
            }
            Rtid::Raw { name, parent } => {
                let mut tup = serializer.serialize_tuple(3)?;
                tup.serialize_element(&OverrideByte(3))?;
                tup.serialize_element(name)?;
                tup.serialize_element(parent)?;
                tup.end()
            }
        }
    }
}

struct OverrideByte(u8);
impl Serialize for OverrideByte {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_unit_variant("OverrideByte", self.0 as u32, "")
    }
}

// ================= RTON VALUE =================

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
                    rtid.serialize(serializer)
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
                    && let Ok(blob) = BinaryBlob::from_str(value) {
                        return Ok(RtonValue::Binary(blob));
                    }
                if value.starts_with("RTID(")
                    && let Ok(rtid) = Rtid::from_str(value) {
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
