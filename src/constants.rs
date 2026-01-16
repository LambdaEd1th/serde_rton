use num_enum::TryFromPrimitive;

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
