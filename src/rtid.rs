use crate::error::Error;
use regex::Regex;
use serde::{Serialize, ser};
use std::fmt;
use std::str::FromStr;
use std::sync::OnceLock;

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
            Rtid::Raw { name, parent } => write!(f, "RTID({}@{})", parent, name),
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
            Rtid::Uid {
                group,
                id,
                obj,
                name,
            } => {
                use serde::ser::SerializeTuple;
                if let Some(n) = name {
                    let mut tup = serializer.serialize_tuple(5)?;
                    tup.serialize_element(&OverrideByte(2))?;
                    tup.serialize_element(group)?;
                    tup.serialize_element(id)?;
                    tup.serialize_element(obj)?;
                    tup.serialize_element(n)?;
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
                use serde::ser::SerializeTuple;
                let mut tup = serializer.serialize_tuple(3)?;
                tup.serialize_element(&OverrideByte(3))?;
                tup.serialize_element(parent)?;
                tup.serialize_element(name)?;
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
