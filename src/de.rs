use byteorder::{LittleEndian, ReadBytesExt};
use integer_encoding::VarIntReader;
use serde::de::{self, Deserialize, DeserializeOwned};
use simple_rijndael::impls::RijndaelCbc;
use simple_rijndael::paddings::ZeroPadding;
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

// Helper: Validate RTON Header and Version, or Decrypt if encrypted matching key is provided
// Returns:
// - Ok(Some(Vec<u8>)) if encrypted and successfully decrypted (reader consumed).
// - Ok(None) if standard RTON file (reader advanced past header/version).
// Helper: Validate RTON Header and Version, or Decrypt if encrypted matching key is provided
// Returns:
// - Ok(Some(Vec<u8>)) if encrypted and successfully decrypted (reader consumed).
// - Ok(None) if standard RTON file (reader advanced past header/version).
fn validate_header_and_decrypt<R: Read>(
    reader: &mut R,
    key_seed: Option<&str>,
) -> Result<Option<Vec<u8>>> {
    let mut header_start = [0u8; 2];
    reader.read_exact(&mut header_start)?;

    // Check for Encrypted Header (u16 0x010 LE -> [0x10, 0x00])
    if header_start == [0x10, 0x00] {
        let key_str = key_seed.ok_or(Error::MissingKey)?;

        // Derive Key and IV from key_str (MD5)
        let digest = md5::compute(key_str).0; // [u8; 16]
        let hex_string = hex::encode(digest); // String (32 chars)
        let hex_bytes = hex_string.as_bytes(); // &[u8] (32 bytes)

        let key = hex_bytes.to_vec(); // 32 bytes (256 bits)
        let iv = hex_bytes[4..28].to_vec(); // 24 bytes (192 bits)
        let block_size = 24;

        // Decrypt
        let mut cipher_text = Vec::new();
        reader.read_to_end(&mut cipher_text)?;

        let cipher = RijndaelCbc::<ZeroPadding>::new(&key, block_size)
            .map_err(|e| Error::DecryptionError(format!("Cipher init failed: {:?}", e)))?;

        let decrypted = cipher
            .decrypt(&iv, cipher_text)
            .map_err(|e| Error::DecryptionError(format!("Decryption failed: {:?}", e)))?;

        // Validating the inner content logic is handled by the caller recursively calling standard methods
        return Ok(Some(decrypted));
    }

    // Not encrypted header, check if it matches first 2 bytes of RTON ("RT" -> 0x52 0x54)
    if header_start != FILE_HEADER[0..2] {
        return Err(Error::InvalidHeader);
    }

    // Read remaining 2 bytes of RTON header ("ON")
    let mut header_end = [0u8; 2];
    reader.read_exact(&mut header_end)?;
    if header_end != FILE_HEADER[2..4] {
        return Err(Error::InvalidHeader);
    }

    let ver = reader.read_u32::<LittleEndian>()?;
    if ver != FILE_VERSION {
        return Err(Error::Message(format!("Unsupported version: {}", ver)));
    }
    Ok(None)
}

/// Deserializes a RTON byte slice into a type.
pub fn from_bytes<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T> {
    let mut cursor = Cursor::new(bytes);
    // Passing None for key means if it encounters encrypted header, it returns MissingKey error.
    let check = validate_header_and_decrypt(&mut cursor, None)?;
    if check.is_some() {
        // Should be unreachable because validate_header_and_decrypt returns Err if key is None and header is encrypted.
        return Err(Error::Message("Unexpected decryption in from_bytes".into()));
    }

    let mut deserializer = RtonDeserializer::new(cursor);
    let value = T::deserialize(&mut deserializer)?;
    Ok(value)
}

/// Deserializes a RTON byte slice into a type, with optional decryption key.
/// Note: Requires T to be DeserializeOwned because decryption produces new owned data.
pub fn from_bytes_with_key<T: DeserializeOwned>(bytes: &[u8], key_seed: Option<&str>) -> Result<T> {
    let mut cursor = Cursor::new(bytes);
    let check = validate_header_and_decrypt(&mut cursor, key_seed)?;

    if let Some(decrypted) = check {
        return from_reader(Cursor::new(decrypted));
    }

    let mut deserializer = RtonDeserializer::new(cursor);
    let value = T::deserialize(&mut deserializer)?;
    Ok(value)
}

/// Deserializes an IO stream into a type.
pub fn from_reader<R: Read + Seek, T: DeserializeOwned>(reader: R) -> Result<T> {
    from_reader_with_key(reader, None)
}

/// Deserializes an IO stream into a type, with optional decryption key.
pub fn from_reader_with_key<R: Read + Seek, T: DeserializeOwned>(
    mut reader: R,
    key_seed: Option<&str>,
) -> Result<T> {
    let check = validate_header_and_decrypt(&mut reader, key_seed)?;
    if let Some(decrypted) = check {
        return from_reader(Cursor::new(decrypted));
    }

    let mut deserializer = RtonDeserializer::new(reader);
    let value = T::deserialize(&mut deserializer)?;
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

            RtonIdentifier::Int8Zero => visitor.visit_u8(0),
            RtonIdentifier::UIntZero => visitor.visit_i8(0),
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

            RtonIdentifier::VarIntU32 | RtonIdentifier::VarIntU32Alt => {
                visitor.visit_u32(self.reader.read_varint::<u32>()?)
            }
            RtonIdentifier::VarIntU64 | RtonIdentifier::VarIntU64Alt => {
                visitor.visit_u64(self.reader.read_varint::<u64>()?)
            }
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
            RtonIdentifier::BoolX1 => {
                let b = self.reader.read_u8()?;
                visitor.visit_bool(b != 0)
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
        deserialize_option, deserialize_unit, deserialize_seq, deserialize_map,
        deserialize_identifier, deserialize_ignored_any,
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
