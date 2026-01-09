# serde_rton

A high-performance Rust library for serializing and deserializing the **RTON** (Reflection Object Notation) data format using the [Serde](https://serde.rs/) framework.

RTON is a binary data format commonly used in the PopCap framework games (e.g., *Plants vs. Zombies 2*). This library allows you to seamlessly convert RTON files to Rust structs, JSON, YAML, or modify them dynamically using an AST (Abstract Syntax Tree).

## 🚀 Features

* **Full Serde Support**: Implements `Serializer` and `Deserializer` traits.
* **Zero-Copy Deserialization**: Efficient parsing logic.
* **RTON Specifics**:
    * Handles **VarInt** (LEB128) and **ZigZag** encoding automatically.
    * Supports **String Interning** (Reference counting for strings to save space).
    * Native support for **RTID** (Resource Type IDs) formatting.
    * Supports **Binary Blobs** (Tag `0x87`).
* **Dynamic AST (`RtonValue`)**:
    * Preserves insertion order of object keys (Essential for game data).
    * Handles **Binary** data natively.
    * **JSON/YAML Compatibility**: Automatically converts large `UInt64` (UIDs) to Hex Strings (`"0x..."`) to prevent precision loss in web formats.

## 📦 Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_rton = { path = "." } # If using locally
# or git dependency
# serde_rton = { git = "[https://github.com/LambdaEd1th/serde_rton](https://github.com/LambdaEd1th/serde_rton)" }

```

## 📖 Usage

### 1. Deserializing to a Strongly Typed Struct

If you know the structure of the RTON file, define a Rust struct:

```rust
use serde::{Deserialize, Serialize};
use serde_rton::from_bytes;

#[derive(Serialize, Deserialize, Debug)]
struct PlayerProfile {
    version: u32,
    name: String,
    coins: i32,
    is_active: bool,
}

fn main() -> serde_rton::Result<()> {
    let data = std::fs::read("profile.rton")?;
    let profile: PlayerProfile = from_bytes(&data)?;
    
    println!("Player: {}", profile.name);
    Ok(())
}

```

### 2. Using `RtonValue` (Dynamic / Generic)

If the structure is unknown, or you want to convert formats (e.g., RTON -> JSON), use `RtonValue`.

```rust
use serde_rton::{from_bytes, to_bytes, RtonValue};

fn main() -> serde_rton::Result<()> {
    let data = std::fs::read("level.rton")?;
    
    // Deserialize to dynamic AST
    let value: RtonValue = from_bytes(&data)?;
    println!("Loaded: {:?}", value);

    // Modify data
    if let RtonValue::Object(ref mut map) = value {
        map.push(("new_field".to_string(), RtonValue::Bool(true)));
    }

    // Serialize back to bytes
    let new_data = to_bytes(&value)?;
    std::fs::write("level_modified.rton", new_data)?;
    
    Ok(())
}

```

### 3. Converting RTON to JSON/YAML

Since `serde_rton` implements standard Serde traits, you can easily transcode formats.

```rust
use serde_rton::{from_bytes, RtonValue};
use std::fs;

fn main() {
    let rton_data = fs::read("test.rton").unwrap();
    let val: RtonValue = from_bytes(&rton_data).unwrap();

    // Convert to JSON
    let json_str = serde_json::to_string_pretty(&val).unwrap();
    println!("{}", json_str);
    
    // Note: UInt64 fields (UIDs) will appear as "0x1234abcd" strings 
    // to ensure compatibility with JSON number limits.
}

```

## 🧩 Data Type Details

| RTON Type | Rust Type | Notes |
| --- | --- | --- |
| `Bool` | `bool` | `0x00` (False), `0x01` (True) |
| `Int8` - `Int64` | `i8` - `i64` | Optimized zero compression supported. |
| `UInt8` - `UInt64` | `u8` - `u64` | `UInt64` serializes as Hex String in `RtonValue`. |
| `VarInt` | `i64`/`u64` | Automatically handled (ZigZag for signed). |
| `String` | `String` | Reference counting (defs/refs) handled internally. |
| `Binary` | `Vec<u8>` | Tag `0x87`. |
| `RTID` | `String` | Formatted as `RTID(uid@path)` or `RTID(0)`. |
| `Array` | `Vec<T>` | Requires known length prefix. |
| `Object` | `Map` | Preserves key order. |

### Special String Handling

* **`*`**: The string `"*"` is optimized to a specific single byte tag (`0x02`) in RTON.
* **RTID**: Strings matching the pattern `RTID(...)` are parsed and serialized into the special RTID binary structure (`0x83`).

## 🛠 Project Structure

* `src/lib.rs`: Crate entry point.
* `src/constants.rs`: Definition of Magic Bytes, Versions, and Tags (`RtonIdentifier`).
* `src/ser.rs`: Implementation of `serde::Serializer`.
* `src/de.rs`: Implementation of `serde::Deserializer`.
* `src/value.rs`: The `RtonValue` enum definition.

## 🧪 Testing

Run unit and integration tests:

```bash
cargo test

```

This includes round-trip tests for JSON and YAML conversion located in the `tests/` directory.

## 📄 License

GPL-3.0 License.

```

```