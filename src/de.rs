use byteorder::{LittleEndian, ReadBytesExt};
use integer_encoding::VarIntReader;
use serde::de::{self, DeserializeOwned};
use std::io::{Read, Seek, SeekFrom};

use crate::error::{Error, Result};
use crate::types::{FILE_FOOTER, FILE_HEADER, FILE_VERSION, RtidIdentifier, RtonIdentifier};

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

// Helper: Read an ASCII string by byte length
fn read_ascii_string<R: Read>(reader: &mut R, len: u64) -> Result<String> {
    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

// Helper: Read exactly `count` UTF-8 characters from stream
fn read_utf8_chars<R: Read>(reader: &mut R, count: u64) -> Result<String> {
    let mut s = String::new();
    for _ in 0..count {
        let mut first_byte = [0u8; 1];
        reader.read_exact(&mut first_byte)?;
        let b = first_byte[0];

        let width = if b & 0x80 == 0 {
            1
        } else if b & 0xE0 == 0xC0 {
            2
        } else if b & 0xF0 == 0xE0 {
            3
        } else if b & 0xF8 == 0xF0 {
            4
        } else {
            return Err(Error::InvalidUtf8StartByte(b));
        };

        let mut char_buf = vec![0u8; width];
        char_buf[0] = b;
        if width > 1 {
            reader.read_exact(&mut char_buf[1..])?;
        }

        let ch = String::from_utf8(char_buf)?;
        s.push_str(&ch);
    }
    Ok(s)
}

/// Validate the standard RTON header: magic "RTON" and version.
/// Advances the reader past the header.
fn validate_header<R: Read>(reader: &mut R) -> Result<()> {
    let mut header = [0u8; 4];
    reader.read_exact(&mut header)?;

    if header != *FILE_HEADER {
        return Err(Error::InvalidHeader);
    }

    let ver = reader.read_u32::<LittleEndian>()?;
    if ver != FILE_VERSION {
        return Err(Error::Message(format!("Unsupported version: {}", ver)));
    }
    Ok(())
}

fn validate_footer<R: Read>(reader: &mut R) -> Result<()> {
    let mut footer = [0u8; 4];
    match reader.read_exact(&mut footer) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(Error::InvalidFooter);
        }
        Err(err) => return Err(Error::Io(err)),
    }

    if footer != FILE_FOOTER {
        return Err(Error::InvalidFooter);
    }

    let mut trailing = Vec::new();
    reader.read_to_end(&mut trailing)?;
    if trailing.iter().any(|&byte| byte != 0) {
        return Err(Error::Message(
            "Unexpected trailing data after DONE footer".into(),
        ));
    }

    Ok(())
}

/// Deserializes a RTON byte slice into a type.
pub fn from_bytes<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    from_reader(std::io::Cursor::new(bytes))
}

/// Deserializes an IO stream into a type.
pub fn from_reader<R: Read + Seek, T: DeserializeOwned>(mut reader: R) -> Result<T> {
    validate_header(&mut reader)?;

    let mut deserializer = RtonDeserializer::new(reader);
    let value = T::deserialize(&mut deserializer)?;
    validate_footer(&mut deserializer.reader)?;
    Ok(value)
}

// Macro to generate simple forwarding deserialize methods
macro_rules! forward_to_deserialize_any {
    ($($method:ident),* $(,)?) => {
        $(
            fn $method<V>(self, visitor: V) -> Result<V::Value>
            where
                V: de::Visitor<'de>,
            {
                self.deserialize_any(visitor)
            }
        )*
    };
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
            RtonIdentifier::StrNull => visitor.visit_str("*"),

            RtonIdentifier::Int8Zero => visitor.visit_i8(0),
            RtonIdentifier::UIntZero => visitor.visit_u8(0),
            RtonIdentifier::Int16Zero => visitor.visit_i16(0),
            RtonIdentifier::UInt16Zero => visitor.visit_u16(0),
            RtonIdentifier::Int32Zero => visitor.visit_i32(0),
            RtonIdentifier::UInt32Zero => visitor.visit_u32(0),
            RtonIdentifier::Int64Zero => visitor.visit_i64(0),
            RtonIdentifier::UInt64Zero => visitor.visit_u64(0),

            RtonIdentifier::Int8 => visitor.visit_i8(self.reader.read_i8()?),
            RtonIdentifier::UInt8 => visitor.visit_u8(self.reader.read_u8()?),
            RtonIdentifier::Int16 => visitor.visit_i16(read_primitive!(self.reader, read_i16)),
            RtonIdentifier::UInt16 => visitor.visit_u16(read_primitive!(self.reader, read_u16)),
            RtonIdentifier::Int32 => visitor.visit_i32(read_primitive!(self.reader, read_i32)),
            RtonIdentifier::UInt32 => visitor.visit_u32(read_primitive!(self.reader, read_u32)),
            RtonIdentifier::Int64 => visitor.visit_i64(read_primitive!(self.reader, read_i64)),
            RtonIdentifier::UInt64 => visitor.visit_u64(read_primitive!(self.reader, read_u64)),

            RtonIdentifier::VarIntU32 => {
                let value = self.reader.read_varint::<u32>()?;
                if let Ok(value) = i32::try_from(value) {
                    visitor.visit_i32(value)
                } else {
                    visitor.visit_u32(value)
                }
            }
            RtonIdentifier::VarIntU32Alt => visitor.visit_u32(self.reader.read_varint::<u32>()?),
            RtonIdentifier::VarIntU64 => {
                let value = self.reader.read_varint::<u64>()?;
                if let Ok(value) = i64::try_from(value) {
                    visitor.visit_i64(value)
                } else {
                    visitor.visit_u64(value)
                }
            }
            RtonIdentifier::VarIntU64Alt => visitor.visit_u64(self.reader.read_varint::<u64>()?),
            RtonIdentifier::VarIntI32 | RtonIdentifier::VarIntI32Alt => {
                visitor.visit_i32(self.reader.read_varint::<i32>()?)
            }
            RtonIdentifier::VarIntI64 | RtonIdentifier::VarIntI64Alt => {
                visitor.visit_i64(self.reader.read_varint::<i64>()?)
            }

            RtonIdentifier::Float => visitor.visit_f32(read_primitive!(self.reader, read_f32)),
            RtonIdentifier::FloatZero => visitor.visit_f32(0.0),
            RtonIdentifier::Double => visitor.visit_f64(read_primitive!(self.reader, read_f64)),
            RtonIdentifier::DoubleZero => visitor.visit_f64(0.0),

            RtonIdentifier::StrAsciiDirect => {
                let len: u64 = self.reader.read_varint()?;
                visitor.visit_string(read_ascii_string(&mut self.reader, len)?)
            }
            RtonIdentifier::StrAsciiDef => {
                let len: u64 = self.reader.read_varint()?;
                let s = read_ascii_string(&mut self.reader, len)?;
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
                let char_count: u64 = self.reader.read_varint()?;
                let byte_len: u64 = self.reader.read_varint()?;
                let s = read_utf8_chars(&mut self.reader, char_count)?;
                if s.len() as u64 != byte_len {
                    return Err(Error::StringLengthMismatch {
                        expected: byte_len,
                        actual: s.len() as u64,
                    });
                }
                visitor.visit_string(s)
            }
            RtonIdentifier::StrUtf8Def => {
                let char_count: u64 = self.reader.read_varint()?;
                let byte_len: u64 = self.reader.read_varint()?;
                let s = read_utf8_chars(&mut self.reader, char_count)?;
                if s.len() as u64 != byte_len {
                    return Err(Error::StringLengthMismatch {
                        expected: byte_len,
                        actual: s.len() as u64,
                    });
                }
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

            RtonIdentifier::BinaryBlob => {
                let _ = self.reader.read_u8()?;
                let len = self.reader.read_varint()?;
                let hex_str = read_ascii_string(&mut self.reader, len)?;
                let _ = self.reader.read_varint::<u64>()?;

                let mut bytes = Vec::with_capacity(hex_str.len() / 2);
                for i in (0..hex_str.len()).step_by(2) {
                    if i + 2 <= hex_str.len() {
                        let byte = u8::from_str_radix(&hex_str[i..i + 2], 16)?;
                        bytes.push(byte);
                    }
                }
                visitor.visit_byte_buf(bytes)
            }

            RtonIdentifier::Rtid => {
                let sub_id = RtidIdentifier::try_from(self.reader.read_u8()?)
                    .map_err(|_| Error::UnknownRtidSubId(0))?;
                match sub_id {
                    RtidIdentifier::Zero => visitor.visit_str("RTID(0)"),
                    RtidIdentifier::UidNoString => {
                        let v2: u64 = self.reader.read_varint()?;
                        let v1: u64 = self.reader.read_varint()?;
                        let x = self.reader.read_u32::<LittleEndian>()?;
                        visitor.visit_string(format!("RTID({:x}.{:x}.{:08x}@)", v1, v2, x))
                    }
                    RtidIdentifier::Uid => {
                        let char_count: u64 = self.reader.read_varint()?;
                        let byte_len: u64 = self.reader.read_varint()?;
                        let name = read_utf8_chars(&mut self.reader, char_count)?;
                        if name.len() as u64 != byte_len {
                            return Err(Error::StringLengthMismatch {
                                expected: byte_len,
                                actual: name.len() as u64,
                            });
                        }

                        let v2: u64 = self.reader.read_varint()?;
                        let v1: u64 = self.reader.read_varint()?;
                        let x = self.reader.read_u32::<LittleEndian>()?;
                        visitor.visit_string(format!("RTID({:x}.{:x}.{:08x}@{})", v1, v2, x, name))
                    }
                    RtidIdentifier::String => {
                        let char_count1: u64 = self.reader.read_varint()?;
                        let bl1: u64 = self.reader.read_varint()?;
                        let s1 = read_utf8_chars(&mut self.reader, char_count1)?;
                        if s1.len() as u64 != bl1 {
                            return Err(Error::StringLengthMismatch {
                                expected: bl1,
                                actual: s1.len() as u64,
                            });
                        }

                        let char_count2: u64 = self.reader.read_varint()?;
                        let bl2: u64 = self.reader.read_varint()?;
                        let s2 = read_utf8_chars(&mut self.reader, char_count2)?;
                        if s2.len() as u64 != bl2 {
                            return Err(Error::StringLengthMismatch {
                                expected: bl2,
                                actual: s2.len() as u64,
                            });
                        }

                        visitor.visit_string(format!("RTID({}@{})", s1, s2))
                    }
                }
            }
            RtonIdentifier::RtidZero => visitor.visit_str("RTID(0)"),

            RtonIdentifier::ArrayStart => {
                if self.reader.read_u8()? != RtonIdentifier::ArrayCapacity as u8 {
                    return Err(Error::ArrayStartMismatch);
                }
                let capacity: u64 = self.reader.read_varint()?;
                visitor.visit_seq(RtonSeqAccess::new(self, capacity as usize))
            }
            RtonIdentifier::ObjectStart => visitor.visit_map(RtonMapAccess::new(self)),
            RtonIdentifier::ObjectStartCompact => visitor.visit_map(RtonMapAccess::new(self)),

            RtonIdentifier::ArrayStartCompact => {
                // Compact arrays use the same capacity-prefix convention
                // as standard arrays (0xFD + varint).
                if self.reader.read_u8()? != RtonIdentifier::ArrayCapacity as u8 {
                    return Err(Error::ArrayStartMismatch);
                }
                let capacity: u64 = self.reader.read_varint()?;
                visitor.visit_seq(RtonSeqAccess::new(self, capacity as usize))
            }

            RtonIdentifier::RtidCompact => {
                // Compact RTID — same sub-id structure as regular RTID (0x83).
                let sub_id = RtidIdentifier::try_from(self.reader.read_u8()?)
                    .map_err(|_| Error::UnknownRtidSubId(0))?;
                match sub_id {
                    RtidIdentifier::Zero => visitor.visit_str("RTID(0)"),
                    RtidIdentifier::UidNoString => {
                        let v2: u64 = self.reader.read_varint()?;
                        let v1: u64 = self.reader.read_varint()?;
                        let x = self.reader.read_u32::<LittleEndian>()?;
                        visitor.visit_string(format!("RTID({:x}.{:x}.{:08x}@)", v1, v2, x))
                    }
                    RtidIdentifier::Uid => {
                        let char_count: u64 = self.reader.read_varint()?;
                        let byte_len: u64 = self.reader.read_varint()?;
                        let name = read_utf8_chars(&mut self.reader, char_count)?;
                        if name.len() as u64 != byte_len {
                            return Err(Error::StringLengthMismatch {
                                expected: byte_len,
                                actual: name.len() as u64,
                            });
                        }
                        let v2: u64 = self.reader.read_varint()?;
                        let v1: u64 = self.reader.read_varint()?;
                        let x = self.reader.read_u32::<LittleEndian>()?;
                        visitor.visit_string(format!("RTID({:x}.{:x}.{:08x}@{})", v1, v2, x, name))
                    }
                    RtidIdentifier::String => {
                        let char_count1: u64 = self.reader.read_varint()?;
                        let bl1: u64 = self.reader.read_varint()?;
                        let s1 = read_utf8_chars(&mut self.reader, char_count1)?;
                        if s1.len() as u64 != bl1 {
                            return Err(Error::StringLengthMismatch {
                                expected: bl1,
                                actual: s1.len() as u64,
                            });
                        }
                        let char_count2: u64 = self.reader.read_varint()?;
                        let bl2: u64 = self.reader.read_varint()?;
                        let s2 = read_utf8_chars(&mut self.reader, char_count2)?;
                        if s2.len() as u64 != bl2 {
                            return Err(Error::StringLengthMismatch {
                                expected: bl2,
                                actual: s2.len() as u64,
                            });
                        }
                        visitor.visit_string(format!("RTID({}@{})", s1, s2))
                    }
                }
            }

            RtonIdentifier::BinaryBlobCompact => {
                let len: u64 = self.reader.read_varint()?;
                let mut buf = vec![0u8; len as usize];
                self.reader.read_exact(&mut buf)?;
                visitor.visit_byte_buf(buf)
            }

            RtonIdentifier::BoolCompact => {
                let b = self.reader.read_u8()?;
                visitor.visit_bool(b != 0)
            }

            // Compact string tags — decode same as their non-compact counterparts.
            RtonIdentifier::StrCompactAsciiDef => {
                let len: u64 = self.reader.read_varint()?;
                let s = read_ascii_string(&mut self.reader, len)?;
                self.ref_table_90.push(s.clone());
                visitor.visit_string(s)
            }
            RtonIdentifier::StrCompactAsciiRef => {
                let idx: u64 = self.reader.read_varint()?;
                let s = self
                    .ref_table_90
                    .get(idx as usize)
                    .ok_or(Error::RefIndexOutOfBounds)?
                    .clone();
                visitor.visit_string(s)
            }
            RtonIdentifier::StrCompactUtf8Def => {
                let char_count: u64 = self.reader.read_varint()?;
                let byte_len: u64 = self.reader.read_varint()?;
                let s = read_utf8_chars(&mut self.reader, char_count)?;
                if s.len() as u64 != byte_len {
                    return Err(Error::StringLengthMismatch {
                        expected: byte_len,
                        actual: s.len() as u64,
                    });
                }
                self.ref_table_92.push(s.clone());
                visitor.visit_string(s)
            }
            RtonIdentifier::StrCompactUtf8Ref => {
                let idx: u64 = self.reader.read_varint()?;
                let s = self
                    .ref_table_92
                    .get(idx as usize)
                    .ok_or(Error::RefIndexOutOfBounds)?
                    .clone();
                visitor.visit_string(s)
            }
            // Paired compact variants — decode as their base counterparts.
            //
            // Hopper evidence (sub_1024e7b5c): 0xB4–0xB7 are selected when the
            // compact transcoder tracks auxiliary offsets.  The string payload
            // is identical to the base variant; the aux data is tracked
            // out-of-band by the transcoder state.  For a generic decoder,
            // treating them as their base counterparts is safe.
            RtonIdentifier::StrCompactPair1 => {
                // 0xB4 ≈ 0xB0 (StrCompactAsciiDef)
                let len: u64 = self.reader.read_varint()?;
                let s = read_ascii_string(&mut self.reader, len)?;
                self.ref_table_90.push(s.clone());
                visitor.visit_string(s)
            }
            RtonIdentifier::StrCompactPair2 => {
                // 0xB5 ≈ 0xB1 (StrCompactAsciiRef)
                let idx: u64 = self.reader.read_varint()?;
                let s = self
                    .ref_table_90
                    .get(idx as usize)
                    .ok_or(Error::RefIndexOutOfBounds)?
                    .clone();
                visitor.visit_string(s)
            }
            RtonIdentifier::StrCompactPair3 => {
                // 0xB6 ≈ 0xB2 (StrCompactUtf8Def)
                let char_count: u64 = self.reader.read_varint()?;
                let byte_len: u64 = self.reader.read_varint()?;
                let s = read_utf8_chars(&mut self.reader, char_count)?;
                if s.len() as u64 != byte_len {
                    return Err(Error::StringLengthMismatch {
                        expected: byte_len,
                        actual: s.len() as u64,
                    });
                }
                self.ref_table_92.push(s.clone());
                visitor.visit_string(s)
            }
            RtonIdentifier::StrCompactPair4 => {
                // 0xB7 ≈ 0xB3 (StrCompactUtf8Ref)
                let idx: u64 = self.reader.read_varint()?;
                let s = self
                    .ref_table_92
                    .get(idx as usize)
                    .ok_or(Error::RefIndexOutOfBounds)?
                    .clone();
                visitor.visit_string(s)
            }
            _ => Err(Error::UnknownTag(tag_byte)),
        }
    }

    forward_to_deserialize_any! {
        deserialize_bool,
        deserialize_i8, deserialize_i16, deserialize_i32, deserialize_i64,
        deserialize_u8, deserialize_u16, deserialize_u32, deserialize_u64,
        deserialize_f32, deserialize_f64,
        deserialize_char, deserialize_str, deserialize_string,
        deserialize_bytes, deserialize_byte_buf,
        deserialize_unit, deserialize_seq, deserialize_map,
        deserialize_identifier, deserialize_ignored_any,
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        // PvZ2 represents JSON null as RtidZero (0x84).
        // Peek at the next tag; if it's RtidZero, consume it and return None.
        let pos = self.reader.stream_position().map_err(Error::Io)?;
        let tag_byte = self.reader.read_u8().map_err(Error::Io)?;
        if tag_byte == RtonIdentifier::RtidZero as u8 {
            visitor.visit_none()
        } else {
            self.reader.seek(SeekFrom::Start(pos)).map_err(Error::Io)?;
            visitor.visit_some(self)
        }
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
        Err(Error::Message(
            "RTON does not support enum deserialization".into(),
        ))
    }
}

struct RtonSeqAccess<'a, 'de, R> {
    de: &'a mut RtonDeserializer<'de, R>,
    remaining_capacity: usize,
}
impl<'a, 'de, R: Read + Seek> RtonSeqAccess<'a, 'de, R> {
    fn new(de: &'a mut RtonDeserializer<'de, R>, capacity: usize) -> Self {
        Self {
            de,
            remaining_capacity: capacity,
        }
    }
}
impl<'de, 'a, R: Read + Seek> de::SeqAccess<'de> for RtonSeqAccess<'a, 'de, R> {
    type Error = Error;
    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: de::DeserializeSeed<'de>,
    {
        let mut buf = [0u8; 1];
        self.de.reader.read_exact(&mut buf)?;
        if buf[0] == RtonIdentifier::ArrayEnd as u8 {
            if self.remaining_capacity != 0 {
                return Err(Error::ArrayLengthMismatch);
            }
            return Ok(None);
        }
        if self.remaining_capacity == 0 {
            return Err(Error::ArrayOverflow);
        }
        self.de.reader.seek(SeekFrom::Current(-1))?;
        self.remaining_capacity -= 1;
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
        if buf[0] == RtonIdentifier::ObjectEnd as u8 {
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
