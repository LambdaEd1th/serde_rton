# serde_rton

A high-performance Rust library for serializing and deserializing the **RTON** (Reflection Object Notation) data format using the [Serde](https://serde.rs/) framework.

RTON is a binary data format commonly used in PopCap framework games (e.g., *Plants vs. Zombies 2*). This library provides a robust interface to parse RTON files into Rust structs or dynamic ASTs, and serialize them back with high fidelity.

## 🚀 Features

* **Full Serde Integration**: Implements `Serializer` and `Deserializer` traits efficiently.
* **High-Fidelity Types**:
    * Supports specific integer widths (`i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64`) to preserve the exact binary layout.
    * Distinguishes between Fixed-width integers and Variable-length integers (VarInts).
* **RTON Specific Optimizations**:
    * **Zero-Value Optimization**: Automatically uses dedicated zero-tags (e.g., `Int32Zero`) to save space.
    * **String Interning**: Handles reference counting for strings (`StrAsciiRef`, `StrUtf8Ref`).
    * **Native RTID Support**: Parses and serializes Resource Type IDs (`0x83`).
    * **Order Preservation**: Object keys are maintained in their insertion order (critical for binary game data stability).
* **Zero-Copy Deserialization**: Capable of borrowing string slices where possible (optimization pending implementation).

## 📦 Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_rton = { path = "." } # If using locally
# or via git:
# serde_rton = { git = "[https://github.com/LambdaEd1th/serde_rton](https://github.com/LambdaEd1th/serde_rton)" }

```

## 📖 Usage

### 1. Strongly Typed Deserialization

Define a Rust struct that matches the RTON file structure.

```rust
use serde::{Deserialize, Serialize};
use serde_rton::from_bytes;

#[derive(Serialize, Deserialize, Debug)]
struct LevelData {
    version: u32,
    #[serde(rename = "objclass")]
    class_name: String,
    props: LevelProps,
}

#[derive(Serialize, Deserialize, Debug)]
struct LevelProps {
    zombies: Vec<String>,
    waves: i32,
}

fn main() -> serde_rton::Result<()> {
    let data = std::fs::read("level.rton")?;
    let level: LevelData = from_bytes(&data)?;
    
    println!("Loaded Level: {}", level.class_name);
    Ok(())
}

```

### 2. Dynamic AST (`RtonValue`)

Use `RtonValue` when the schema is unknown or when you need to preserve the exact integer types of the original binary file.

```rust
use serde_rton::{from_bytes, to_bytes, RtonValue};

fn main() -> serde_rton::Result<()> {
    let data = std::fs::read("config.rton")?;
    
    // Deserialize to dynamic AST
    let mut value: RtonValue = from_bytes(&data)?;
    
    // Modify the data
    if let RtonValue::Object(ref mut entries) = value {
        // Add a new specific integer type
        entries.push(("new_score".to_string(), RtonValue::Int16(1024)));
        // Add a specialized RTID string
        entries.push(("resource".to_string(), RtonValue::String("RTID(123@path)".to_string())));
    }

    // Serialize back to bytes
    let new_bytes = to_bytes(&value)?;
    std::fs::write("config_edited.rton", new_bytes)?;
    
    Ok(())
}

```

### 3. Converting to JSON/YAML

You can use `serde_json` or `serde_yaml` to convert RTON data for debugging.

> **⚠️ Note on `u64**`: RTON supports full 64-bit unsigned integers. Standard JSON parsers (like in JavaScript) may lose precision for numbers larger than . This library serializes `u64` as native numbers. If you need safe JSON interop for huge IDs, consider wrapping them or post-processing.

```rust
use serde_rton::{from_bytes, RtonValue};

fn main() {
    let rton_data = std::fs::read("data.rton").unwrap();
    let val: RtonValue = from_bytes(&rton_data).unwrap();

    let json_output = serde_json::to_string_pretty(&val).unwrap();
    println!("{}", json_output);
}

```

## 🧩 Data Type Mapping

| RTON Identifier | Rust Type | Notes |
| --- | --- | --- |
| `BoolTrue` / `BoolFalse` | `bool` |  |
| `Int8` / `UInt8` | `u8` / `i8` |  |
| `Int16` / `UInt16` | `i16` / `u16` |  |
| `Int32` / `UInt32` | `i32` / `u32` |  |
| `Int64` / `UInt64` | `i64` / `u64` | Native serialization (8 bytes) |
| `VarInt` | `i64` / `u64` | Variable-length encoding (LEB128/ZigZag) |
| `Float` | `f32` |  |
| `Double` | `f64` |  |
| `String` | `String` | Supports ASCII and UTF-8 interning |
| `BinaryBlob` (0x87) | `Vec<u8>` | Raw byte arrays |
| `RTID` (0x83) | `String` | Formats: `RTID(uid@path)`, `RTID(str@str)`, `RTID(0)` |
| `Array` | `Vec<RtonValue>` | Prefixed with length |
| `Object` | `Vec<(String, RtonValue)>` | Key-Value pairs, preserves order |

## 🛠 Project Structure

* **`src/lib.rs`**: Library entry point and re-exports.
* **`src/constants.rs`**: Definitions for File Headers, Footers, Versions, and the `RtonIdentifier` enum (Tags).
* **`src/value.rs`**: The `RtonValue` enum acting as the AST.
* **`src/ser.rs`**: Implementation of `serde::Serializer`.
* **`src/de.rs`**: Implementation of `serde::Deserializer`.
* **`src/error.rs`**: Custom error types (`serde_rton::Error`).

## 🧪 Testing

The project includes unit tests and integration tests for Round-Trip conversion (RTON -> JSON -> RTON).

```bash
cargo test

```

## 📄 License

This project is licensed under the **GNU General Public License v3.0 (GPL-3.0)**.

```