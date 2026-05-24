use byteorder::{LittleEndian, WriteBytesExt};
use integer_encoding::VarIntWriter;
use serde::{Serialize, ser};
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::io::Write;

use crate::error::{Error, Result};
use crate::types::{FILE_FOOTER, FILE_HEADER, FILE_VERSION, RtonIdentifier};

// === Helper Functions for String Writing ===

fn write_ascii_payload<W: Write>(writer: &mut W, s: &str) -> Result<()> {
    writer.write_varint(s.len() as u64)?;
    writer.write_all(s.as_bytes())?;
    Ok(())
}

fn write_utf8_payload<W: Write>(writer: &mut W, s: &str) -> Result<()> {
    writer.write_varint(s.chars().count() as u64)?;
    writer.write_varint(s.len() as u64)?;
    writer.write_all(s.as_bytes())?;
    Ok(())
}

fn varint_len_u32(mut value: u32) -> usize {
    let mut len = 1;
    while value > 0x7f {
        value >>= 7;
        len += 1;
    }
    len
}

fn varint_len_u64(mut value: u64) -> usize {
    let mut len = 1;
    while value > 0x7f {
        value >>= 7;
        len += 1;
    }
    len
}

fn zigzag_i32(value: i32) -> u32 {
    ((value as u32) << 1) ^ ((value >> 31) as u32)
}

fn zigzag_i64(value: i64) -> u64 {
    ((value as u64) << 1) ^ ((value >> 63) as u64)
}

// === Helper Functions for Header/Footer ===

fn write_header<W: Write>(writer: &mut W) -> Result<()> {
    writer.write_all(FILE_HEADER)?;
    writer.write_u32::<LittleEndian>(FILE_VERSION)?;
    Ok(())
}

fn write_footer<W: Write>(writer: &mut W) -> Result<()> {
    writer.write_all(FILE_FOOTER)?;
    Ok(())
}

// === Serializer Implementation ===

#[derive(PartialEq, Clone, Copy)]
enum PendingVarInt {
    None,
    I32,
    U32,
    I64,
    U64,
}

pub struct RtonSerializer<W> {
    writer: W,
    cache_90: HashMap<String, u32>,
    next_idx_90: u32,
    cache_92: HashMap<String, u32>,
    next_idx_92: u32,
    is_root: bool,
    pending_varint: PendingVarInt,
    pending_rtid: bool,
}

impl<W: Write> RtonSerializer<W> {
    pub fn new(writer: W) -> Self {
        RtonSerializer {
            writer,
            cache_90: HashMap::new(),
            next_idx_90: 0,
            cache_92: HashMap::new(),
            next_idx_92: 0,
            is_root: true,
            pending_varint: PendingVarInt::None,
            pending_rtid: false,
        }
    }

    fn write_interned_string(&mut self, v: &str) -> Result<()> {
        let is_ascii = v.is_ascii();
        if is_ascii {
            if let Some(&idx) = self.cache_90.get(v) {
                self.writer.write_u8(RtonIdentifier::StrAsciiRef as u8)?;
                self.writer.write_varint(idx as u64)?;
            } else {
                self.writer.write_u8(RtonIdentifier::StrAsciiDef as u8)?;
                write_ascii_payload(&mut self.writer, v)?;
                self.cache_90.insert(v.to_string(), self.next_idx_90);
                self.next_idx_90 += 1;
            }
        } else if let Some(&idx) = self.cache_92.get(v) {
            self.writer.write_u8(RtonIdentifier::StrUtf8Ref as u8)?;
            self.writer.write_varint(idx as u64)?;
        } else {
            self.writer.write_u8(RtonIdentifier::StrUtf8Def as u8)?;
            write_utf8_payload(&mut self.writer, v)?;
            self.cache_92.insert(v.to_string(), self.next_idx_92);
            self.next_idx_92 += 1;
        }
        Ok(())
    }
}

/// Serializes the given data structure to a RTON byte vector, with optional encryption key.
pub fn to_bytes<T: Serialize>(value: &T, key_seed: Option<&str>) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    to_writer(&mut data, value, key_seed)?;
    Ok(data)
}

/// Serializes the given data structure as RTON into the IO stream, with optional encryption key.
pub fn to_writer<W: Write, T: Serialize>(
    mut writer: W,
    value: &T,
    key_seed: Option<&str>,
) -> Result<()> {
    if let Some(key_str) = key_seed {
        // Write Encrypted Header (u16 0x010 LE -> [0x10, 0x00])
        writer.write_all(&[0x10, 0x00])?;

        // Serialize content to buffer first
        let mut buffer = Vec::new();
        // Inner serialization writes standard RTON header + content + footer
        // Recursively call to_writer with None key for inner unencrypted content
        to_writer(&mut buffer, value, None)?;

        // Encrypt buffer using shared crypto module
        let encrypted = crate::crypto::encrypt_data(&buffer, Some(key_str))
            .map_err(|e| Error::Message(format!("Encryption failed: {:?}", e)))?;

        writer.write_all(&encrypted)?;
        return Ok(());
    }

    write_header(&mut writer)?;

    // Create serializer borrowing the writer
    {
        let mut serializer = RtonSerializer::new(&mut writer);
        value.serialize(&mut serializer)?;
    }

    write_footer(&mut writer)?;
    Ok(())
}

impl<W: Write> ser::Serializer for &mut RtonSerializer<W> {
    type Ok = ();
    type Error = Error;

    type SerializeSeq = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = ser::Impossible<(), Error>;
    type SerializeTupleVariant = ser::Impossible<(), Error>;
    type SerializeStructVariant = ser::Impossible<(), Error>;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<()> {
        match name {
            "RTID" => {
                self.writer.write_u8(RtonIdentifier::Rtid as u8)?;
                self.pending_rtid = true;
                let result = value.serialize(&mut *self);
                self.pending_rtid = false;
                result
            }
            "VarIntI32" => {
                self.pending_varint = PendingVarInt::I32;
                value.serialize(&mut *self)?;
                self.pending_varint = PendingVarInt::None;
                Ok(())
            }
            "VarIntU32" => {
                self.pending_varint = PendingVarInt::U32;
                value.serialize(&mut *self)?;
                self.pending_varint = PendingVarInt::None;
                Ok(())
            }
            "VarIntI64" => {
                self.pending_varint = PendingVarInt::I64;
                value.serialize(&mut *self)?;
                self.pending_varint = PendingVarInt::None;
                Ok(())
            }
            "VarIntU64" => {
                self.pending_varint = PendingVarInt::U64;
                value.serialize(&mut *self)?;
                self.pending_varint = PendingVarInt::None;
                Ok(())
            }
            _ => value.serialize(self),
        }
    }

    fn serialize_i32(self, v: i32) -> Result<()> {
        if self.pending_varint == PendingVarInt::I32 {
            self.writer.write_u8(RtonIdentifier::VarIntI32 as u8)?;
            self.writer.write_varint(v)?;
            return Ok(());
        }
        if v == 0 {
            self.writer.write_u8(RtonIdentifier::Int32Zero as u8)?;
        } else {
            let raw = v as u32;
            let raw_len = varint_len_u32(raw);
            let zigzag_len = varint_len_u32(zigzag_i32(v));

            if raw_len >= 4 && zigzag_len >= 4 {
                self.writer.write_u8(RtonIdentifier::Int32 as u8)?;
                self.writer.write_i32::<LittleEndian>(v)?;
            } else if zigzag_len < raw_len {
                self.writer.write_u8(RtonIdentifier::VarIntI32 as u8)?;
                self.writer.write_varint(v)?;
            } else {
                self.writer.write_u8(RtonIdentifier::VarIntU32 as u8)?;
                self.writer.write_varint(raw)?;
            }
        }
        Ok(())
    }
    fn serialize_u32(self, v: u32) -> Result<()> {
        if self.pending_rtid {
            self.writer.write_u32::<LittleEndian>(v)?;
            return Ok(());
        }
        if self.pending_varint == PendingVarInt::U32 {
            self.writer.write_u8(RtonIdentifier::VarIntU32Alt as u8)?;
            self.writer.write_varint(v)?;
            return Ok(());
        }
        if v == 0 {
            self.writer.write_u8(RtonIdentifier::UInt32Zero as u8)?;
        } else {
            self.writer.write_u8(RtonIdentifier::UInt32 as u8)?;
            self.writer.write_u32::<LittleEndian>(v)?;
        }
        Ok(())
    }
    fn serialize_i64(self, v: i64) -> Result<()> {
        if self.pending_varint == PendingVarInt::I64 {
            self.writer.write_u8(RtonIdentifier::VarIntI64 as u8)?;
            self.writer.write_varint(v)?;
            return Ok(());
        }
        if v == 0 {
            self.writer.write_u8(RtonIdentifier::Int64Zero as u8)?;
        } else {
            let raw = v as u64;
            let raw_len = varint_len_u64(raw);
            let zigzag_len = varint_len_u64(zigzag_i64(v));

            if raw_len >= 8 && zigzag_len >= 8 {
                self.writer.write_u8(RtonIdentifier::Int64 as u8)?;
                self.writer.write_i64::<LittleEndian>(v)?;
            } else if zigzag_len < raw_len {
                self.writer.write_u8(RtonIdentifier::VarIntI64 as u8)?;
                self.writer.write_varint(v)?;
            } else {
                self.writer.write_u8(RtonIdentifier::VarIntU64 as u8)?;
                self.writer.write_varint(raw)?;
            }
        }
        Ok(())
    }
    fn serialize_u64(self, v: u64) -> Result<()> {
        if self.pending_rtid {
            self.writer.write_varint(v)?;
            return Ok(());
        }
        if self.pending_varint == PendingVarInt::U64 {
            self.writer.write_u8(RtonIdentifier::VarIntU64Alt as u8)?;
            self.writer.write_varint(v)?;
            return Ok(());
        }
        if v == 0 {
            self.writer.write_u8(RtonIdentifier::UInt64Zero as u8)?;
        } else {
            self.writer.write_u8(RtonIdentifier::UInt64 as u8)?;
            self.writer.write_u64::<LittleEndian>(v)?;
        }
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        _variant: &'static str,
    ) -> Result<()> {
        if name == "RTID" && variant_index == 0x84 {
            self.writer.write_u8(RtonIdentifier::RtidZero as u8)?;
            return Ok(());
        }
        self.writer.write_u8(variant_index as u8)?;
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        if self.pending_rtid {
            write_utf8_payload(&mut self.writer, v)?;
            return Ok(());
        }
        if v == "*" {
            self.writer.write_u8(RtonIdentifier::StrNull as u8)?;
            return Ok(());
        }
        if v == "RTID(0)" {
            self.writer.write_u8(RtonIdentifier::RtidZero as u8)?;
            return Ok(());
        }
        self.write_interned_string(v)
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        self.writer.write_u8(RtonIdentifier::BinaryBlob as u8)?;
        self.writer.write_u8(0)?;

        let mut hex_str = String::with_capacity(v.len() * 2);
        for b in v {
            write!(&mut hex_str, "{:02X}", b)?;
        }

        write_ascii_payload(&mut self.writer, &hex_str)?;
        self.writer.write_varint(v.len() as u64)?;
        Ok(())
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        Ok(self)
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        let count = len.ok_or(Error::UnknownLength)?;
        self.writer.write_u8(RtonIdentifier::ArrayStart as u8)?;
        self.writer.write_u8(RtonIdentifier::ArrayCapacity as u8)?;
        self.writer.write_varint(count as u64)?;
        Ok(self)
    }

    fn serialize_none(self) -> Result<()> {
        self.writer.write_u8(RtonIdentifier::StrNull as u8)?;
        Ok(())
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<()> {
        value.serialize(self)
    }
    fn serialize_bool(self, v: bool) -> Result<()> {
        self.writer.write_u8(if v {
            RtonIdentifier::BoolTrue as u8
        } else {
            RtonIdentifier::BoolFalse as u8
        })?;
        Ok(())
    }
    fn serialize_i8(self, v: i8) -> Result<()> {
        if v == 0 {
            self.writer.write_u8(RtonIdentifier::Int8Zero as u8)?;
        } else {
            self.writer.write_u8(RtonIdentifier::Int8 as u8)?;
            self.writer.write_i8(v)?;
        }
        Ok(())
    }
    fn serialize_u8(self, v: u8) -> Result<()> {
        if v == 0 {
            self.writer.write_u8(RtonIdentifier::UIntZero as u8)?;
        } else {
            self.writer.write_u8(RtonIdentifier::UInt8 as u8)?;
            self.writer.write_u8(v)?;
        }
        Ok(())
    }
    fn serialize_i16(self, v: i16) -> Result<()> {
        if v == 0 {
            self.writer.write_u8(RtonIdentifier::Int16Zero as u8)?;
        } else {
            self.writer.write_u8(RtonIdentifier::Int16 as u8)?;
            self.writer.write_i16::<LittleEndian>(v)?;
        }
        Ok(())
    }
    fn serialize_u16(self, v: u16) -> Result<()> {
        if v == 0 {
            self.writer.write_u8(RtonIdentifier::UInt16Zero as u8)?;
        } else {
            self.writer.write_u8(RtonIdentifier::UInt16 as u8)?;
            self.writer.write_u16::<LittleEndian>(v)?;
        }
        Ok(())
    }
    fn serialize_f32(self, v: f32) -> Result<()> {
        if v == 0.0 {
            self.writer.write_u8(RtonIdentifier::FloatZero as u8)?;
        } else {
            self.writer.write_u8(RtonIdentifier::Float as u8)?;
            self.writer.write_f32::<LittleEndian>(v)?;
        }
        Ok(())
    }
    fn serialize_f64(self, v: f64) -> Result<()> {
        if v == 0.0 {
            self.writer.write_u8(RtonIdentifier::DoubleZero as u8)?;
            return Ok(());
        }

        // Downcast to f32. If their string representations are identical, serialize as f32.
        let v32 = v as f32;
        if format!("{}", v) == format!("{}", v32) {
            return self.serialize_f32(v32);
        }

        self.writer.write_u8(RtonIdentifier::Double as u8)?;
        self.writer.write_f64::<LittleEndian>(v)?;
        Ok(())
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        if self.is_root {
            self.is_root = false;
        } else {
            self.writer.write_u8(RtonIdentifier::ObjectStart as u8)?;
        }
        Ok(self)
    }
    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        if self.is_root {
            self.is_root = false;
        } else {
            self.writer.write_u8(RtonIdentifier::ObjectStart as u8)?;
        }
        Ok(self)
    }
    fn serialize_char(self, _v: char) -> Result<()> {
        Err(Error::Message("char not supported".into()))
    }
    fn serialize_unit(self) -> Result<()> {
        self.serialize_none()
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        self.serialize_none()
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _idx: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<()> {
        Err(Error::Message("enum variants not supported".into()))
    }
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Err(Error::Message("tuple structs not supported".into()))
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _idx: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        Err(Error::Message("tuple variants not supported".into()))
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _idx: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Err(Error::Message("struct variants not supported".into()))
    }
}

impl<W: Write> ser::SerializeSeq for &mut RtonSerializer<W> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<()> {
        self.writer.write_u8(RtonIdentifier::ArrayEnd as u8)?;
        Ok(())
    }
}
impl<W: Write> ser::SerializeMap for &mut RtonSerializer<W> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<()> {
        key.serialize(&mut **self)
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<()> {
        self.writer.write_u8(RtonIdentifier::ObjectEnd as u8)?;
        Ok(())
    }
}
impl<W: Write> ser::SerializeStruct for &mut RtonSerializer<W> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        key.serialize(&mut **self)?;
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<()> {
        self.writer.write_u8(RtonIdentifier::ObjectEnd as u8)?;
        Ok(())
    }
}
impl<W: Write> ser::SerializeTuple for &mut RtonSerializer<W> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<()> {
        Ok(())
    }
}
