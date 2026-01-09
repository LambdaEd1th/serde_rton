use byteorder::{LittleEndian, ReadBytesExt};
use integer_encoding::VarIntReader;
use serde::de::{self, Deserialize};
use std::io::{Cursor, Read, Seek, SeekFrom};

use crate::constants::{FILE_HEADER, FILE_VERSION, RtidIdentifier, RtonIdentifier};
use crate::error::{Error, Result};

pub struct RtonDeserializer<'de, R> {
    reader: R,
    ref_table_90: Vec<String>,
    ref_table_92: Vec<String>,
    is_root: bool,
    phantom: std::marker::PhantomData<&'de ()>,
}

impl<'de, R: Read> RtonDeserializer<'de, R> {
    pub fn new(reader: R) -> Self {
        RtonDeserializer {
            reader,
            ref_table_90: Vec::new(),
            ref_table_92: Vec::new(),
            is_root: true,
            phantom: std::marker::PhantomData,
        }
    }
}

macro_rules! read_primitive {
    ($reader:expr, $read_fn:ident) => {
        $reader.$read_fn::<LittleEndian>().map_err(Error::Io)?
    };
}

pub fn from_bytes<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T> {
    let mut cursor = Cursor::new(bytes);

    // 1. Header Validation
    let mut header = [0u8; 4];
    cursor.read_exact(&mut header)?;
    if header != FILE_HEADER {
        return Err(Error::InvalidHeader);
    }

    // 2. Version Validation
    let ver = cursor.read_u32::<LittleEndian>()?;
    if ver != FILE_VERSION {
        return Err(Error::Custom(format!(
            "Unsupported RTON version: {}. Expected: {}",
            ver, FILE_VERSION
        )));
    }

    let mut deserializer = RtonDeserializer::new(cursor);
    let value = T::deserialize(&mut deserializer)?;
    Ok(value)
}

impl<'de, R: Read + Seek> de::Deserializer<'de> for &mut RtonDeserializer<'de, R> {
    type Error = Error;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        if self.is_root {
            self.is_root = false;
            return visitor.visit_map(RtonMapAccess::new(self));
        }

        let tag_byte = self.reader.read_u8().map_err(Error::Io)?;
        let tag = RtonIdentifier::try_from(tag_byte).map_err(|_| Error::UnknownTag(tag_byte))?;

        match tag {
            RtonIdentifier::BoolFalse => visitor.visit_bool(false),
            RtonIdentifier::BoolTrue => visitor.visit_bool(true),

            RtonIdentifier::SpecialStar => visitor.visit_str("*"),

            // Integers Zero Optimization
            RtonIdentifier::Int8Zero => visitor.visit_u8(0),
            RtonIdentifier::UIntZero => visitor.visit_i8(0),

            RtonIdentifier::Int16Zero => visitor.visit_i16(0),
            RtonIdentifier::UInt16Zero => visitor.visit_u16(0),
            RtonIdentifier::Int32Zero => visitor.visit_i32(0),
            RtonIdentifier::UInt32Zero => visitor.visit_u32(0),
            RtonIdentifier::Int64Zero => visitor.visit_i64(0),
            RtonIdentifier::UInt64Zero => visitor.visit_u64(0),

            // Integers (Fixed Width)
            RtonIdentifier::Int8 => visitor.visit_u8(self.reader.read_u8()?),
            RtonIdentifier::UInt8 => visitor.visit_i8(self.reader.read_i8()?),

            RtonIdentifier::Int16 => visitor.visit_i16(read_primitive!(self.reader, read_i16)),
            RtonIdentifier::UInt16 => visitor.visit_u16(read_primitive!(self.reader, read_u16)),
            RtonIdentifier::Int32 => visitor.visit_i32(read_primitive!(self.reader, read_i32)),
            RtonIdentifier::UInt32 => visitor.visit_u32(read_primitive!(self.reader, read_u32)),
            RtonIdentifier::Int64 => visitor.visit_i64(read_primitive!(self.reader, read_i64)),
            RtonIdentifier::UInt64 => visitor.visit_u64(read_primitive!(self.reader, read_u64)),

            // Integers VarInt
            RtonIdentifier::VarIntU32 | RtonIdentifier::VarIntU32Alternative => {
                visitor.visit_u32(self.reader.read_varint::<u32>()?)
            }
            RtonIdentifier::VarIntU64 | RtonIdentifier::VarIntU64Alternative => {
                visitor.visit_u64(self.reader.read_varint::<u64>()?)
            }
            RtonIdentifier::VarIntI32 | RtonIdentifier::VarIntI32Alternative => {
                visitor.visit_i32(self.reader.read_varint::<i32>()?)
            }
            RtonIdentifier::VarIntI64 | RtonIdentifier::VarIntI64Alternative => {
                visitor.visit_i64(self.reader.read_varint::<i64>()?)
            }

            RtonIdentifier::Float => visitor.visit_f32(read_primitive!(self.reader, read_f32)),
            RtonIdentifier::FloatZero => visitor.visit_f32(0.0),
            RtonIdentifier::Double => visitor.visit_f64(read_primitive!(self.reader, read_f64)),
            RtonIdentifier::DoubleZero => visitor.visit_f64(0.0),

            // Strings
            RtonIdentifier::StrAsciiDirect => {
                let len: u64 = self.reader.read_varint()?;
                let mut buf = vec![0u8; len as usize];
                self.reader.read_exact(&mut buf)?;
                let s = String::from_utf8_lossy(&buf).into_owned();
                visitor.visit_string(s)
            }
            RtonIdentifier::StrAsciiDef => {
                let len: u64 = self.reader.read_varint()?;
                let mut buf = vec![0u8; len as usize];
                self.reader.read_exact(&mut buf)?;
                let s = String::from_utf8_lossy(&buf).into_owned();
                self.ref_table_90.push(s.clone());
                visitor.visit_string(s)
            }
            RtonIdentifier::StrAsciiRef => {
                let idx: u64 = self.reader.read_varint()?;
                let s = self
                    .ref_table_90
                    .get(idx as usize)
                    .ok_or(Error::RefIndexOutOfBounds)?
                    .clone();
                visitor.visit_string(s)
            }
            RtonIdentifier::StrUtf8Direct => {
                let _char_len: u64 = self.reader.read_varint()?;
                let byte_len: u64 = self.reader.read_varint()?;
                let mut buf = vec![0u8; byte_len as usize];
                self.reader.read_exact(&mut buf)?;
                let s = String::from_utf8(buf)?;
                visitor.visit_string(s)
            }
            RtonIdentifier::StrUtf8Def => {
                let _char_len: u64 = self.reader.read_varint()?;
                let byte_len: u64 = self.reader.read_varint()?;
                let mut buf = vec![0u8; byte_len as usize];
                self.reader.read_exact(&mut buf)?;
                let s = String::from_utf8(buf)?;
                self.ref_table_92.push(s.clone());
                visitor.visit_string(s)
            }
            RtonIdentifier::StrUtf8Ref => {
                let idx: u64 = self.reader.read_varint()?;
                let s = self
                    .ref_table_92
                    .get(idx as usize)
                    .ok_or(Error::RefIndexOutOfBounds)?
                    .clone();
                visitor.visit_string(s)
            }

            // Binary (0x87)
            RtonIdentifier::BinaryBlob => {
                let len: u64 = self.reader.read_varint()?;
                let mut buf = vec![0u8; len as usize];
                self.reader.read_exact(&mut buf)?;
                visitor.visit_byte_buf(buf)
            }

            // RTID (0x83)
            RtonIdentifier::Rtid => {
                let sub_id_byte = self.reader.read_u8()?;
                let sub_id = RtidIdentifier::try_from(sub_id_byte)
                    .map_err(|_| Error::UnknownRtidSubId(sub_id_byte))?;

                match sub_id {
                    RtidIdentifier::Zero => visitor.visit_str("RTID(0)"),

                    // Added support for 0x01 (UidNoString)
                    RtidIdentifier::UidNoString => {
                        // Read 2 VarInts then 1 UInt32LE.
                        // Order is val12, val11, x161.
                        // Format is val11.val12.x161@ (empty string)
                        let val12: u64 = self.reader.read_varint()?;
                        let val11: u64 = self.reader.read_varint()?;
                        let x161 = self.reader.read_u32::<LittleEndian>()?;

                        let formatted = format!("RTID({:x}.{:x}.{:08x}@)", val11, val12, x161);
                        visitor.visit_string(formatted)
                    }

                    RtidIdentifier::Uid => {
                        // 0x02
                        let _logic_len: u64 = self.reader.read_varint()?;
                        let byte_len: u64 = self.reader.read_varint()?;
                        let mut buf = vec![0u8; byte_len as usize];
                        self.reader.read_exact(&mut buf)?;
                        let str_val = String::from_utf8(buf)?;

                        let uid_1: u64 = self.reader.read_varint()?;
                        let uid_2: u64 = self.reader.read_varint()?;
                        let uid_3 = self.reader.read_u32::<LittleEndian>()?;

                        let formatted =
                            format!("RTID({:x}.{:x}.{:08x}@{})", uid_2, uid_1, uid_3, str_val);
                        visitor.visit_string(formatted)
                    }
                    RtidIdentifier::String => {
                        // 0x03
                        let _len1: u64 = self.reader.read_varint()?;
                        let size1: u64 = self.reader.read_varint()?;
                        let mut buf1 = vec![0u8; size1 as usize];
                        self.reader.read_exact(&mut buf1)?;
                        let str1 = String::from_utf8(buf1)?;

                        let _len2: u64 = self.reader.read_varint()?;
                        let size2: u64 = self.reader.read_varint()?;
                        let mut buf2 = vec![0u8; size2 as usize];
                        self.reader.read_exact(&mut buf2)?;
                        let str2 = String::from_utf8(buf2)?;

                        let formatted = format!("RTID({}@{})", str2, str1);
                        visitor.visit_string(formatted)
                    }
                }
            }

            RtonIdentifier::Null => visitor.visit_none(),

            RtonIdentifier::ArrayStart => {
                let marker = self.reader.read_u8()?;
                if marker != RtonIdentifier::ArraySize as u8 {
                    return Err(Error::ArrayStartMismatch);
                }
                let len: u64 = self.reader.read_varint()?;
                visitor.visit_seq(RtonSeqAccess::new(self, len as usize))
            }

            RtonIdentifier::ObjectStart => visitor.visit_map(RtonMapAccess::new(self)),

            RtonIdentifier::ObjectStartX1
            | RtonIdentifier::ArrayStartX1
            | RtonIdentifier::StrNativeX1
            | RtonIdentifier::StrNativeX2
            | RtonIdentifier::StrNativeX3
            | RtonIdentifier::StrUnicodeX1
            | RtonIdentifier::StrUnicodeX2
            | RtonIdentifier::StrNativeOrUnicodeX1
            | RtonIdentifier::StrNativeOrUnicodeX2
            | RtonIdentifier::StrNativeOrUnicodeX3
            | RtonIdentifier::StrNativeOrUnicodeX4
            | RtonIdentifier::StrBinaryBlobX1
            | RtonIdentifier::BoolX1 => Err(Error::UnsupportedExtendedTag(format!("{:?}", tag))),

            RtonIdentifier::ArraySize | RtonIdentifier::ArrayEnd | RtonIdentifier::ObjectEnd => {
                Err(Error::UnexpectedMarker(format!("{:?}", tag)))
            }
        }
    }

    // ... Forwarding methods ...
    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }
    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }
    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }
    fn deserialize_byte_buf<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        unimplemented!()
    }
}

struct RtonSeqAccess<'a, 'de, R> {
    de: &'a mut RtonDeserializer<'de, R>,
    remaining: usize,
}
impl<'a, 'de, R: Read + Seek> RtonSeqAccess<'a, 'de, R> {
    fn new(de: &'a mut RtonDeserializer<'de, R>, count: usize) -> Self {
        Self {
            de,
            remaining: count,
        }
    }
}
impl<'de, 'a, R: Read + Seek> de::SeqAccess<'de> for RtonSeqAccess<'a, 'de, R> {
    type Error = Error;
    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: de::DeserializeSeed<'de>,
    {
        if self.remaining == 0 {
            let end = self.de.reader.read_u8()?;
            if end != RtonIdentifier::ArrayEnd as u8 {
                return Err(Error::ArrayEndMismatch);
            }
            return Ok(None);
        }
        self.remaining -= 1;
        seed.deserialize(&mut *self.de).map(Some)
    }
}

struct RtonMapAccess<'a, 'de, R> {
    de: &'a mut RtonDeserializer<'de, R>,
}
impl<'a, 'de, R: Read + Seek> RtonMapAccess<'a, 'de, R> {
    fn new(de: &'a mut RtonDeserializer<'de, R>) -> Self {
        Self { de }
    }
}
impl<'de, 'a, R: Read + Seek> de::MapAccess<'de> for RtonMapAccess<'a, 'de, R> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
    where
        K: de::DeserializeSeed<'de>,
    {
        let mut buf = [0u8; 1];
        self.de.reader.read_exact(&mut buf)?;
        let tag = buf[0];

        if tag == RtonIdentifier::ObjectEnd as u8 {
            return Ok(None);
        }

        self.de.reader.seek(SeekFrom::Current(-1))?;
        seed.deserialize(&mut *self.de).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where
        V: de::DeserializeSeed<'de>,
    {
        seed.deserialize(&mut *self.de)
    }
}
