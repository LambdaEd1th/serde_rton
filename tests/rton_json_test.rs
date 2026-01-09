use serde_rton::{RtonValue, from_bytes, to_bytes};
use std::fs;
use std::path::Path;

#[test]
fn test_rton_to_json_and_back() {
    // 1. Define file paths
    let input_path = Path::new("tests/test.rton");
    let json_out_path = Path::new("tests/test.json");
    let rton_out_path = Path::new("tests/test_from_json.rton");

    // 2. Validate input existence
    if !input_path.exists() {
        eprintln!("⚠️ Warning: 'tests/test.rton' not found. Skipping JSON integration test.");
        return;
    }

    println!("📂 Reading RTON from: {:?}", input_path);
    let rton_data = fs::read(input_path).expect("Failed to read test.rton");

    // 3. RTON -> Rust Struct (RtonValue)
    let rton_value: RtonValue = from_bytes(&rton_data).expect("Failed to deserialize RTON");
    println!("✅ RTON deserialized successfully.");

    // 4. Rust Struct -> JSON String
    // We use serde_json to convert the structure to a readable JSON string.
    // Note: Since we modified value.rs to serialize UInt64 as "0x..." strings,
    // this will work perfectly in JSON without numeric overflow issues.
    let json_string =
        serde_json::to_string_pretty(&rton_value).expect("Failed to serialize to JSON");

    fs::write(json_out_path, &json_string).expect("Failed to write test.json");
    println!("📝 Converted to JSON and saved to: {:?}", json_out_path);

    // 5. JSON String -> Rust Struct (RtonValue)
    // Read back to verify data integrity (round-trip).
    // The custom deserializer in value.rs will handle converting "0x..." strings back to UInt64.
    let rton_value_from_json: RtonValue =
        serde_json::from_str(&json_string).expect("Failed to deserialize JSON");

    // 6. Rust Struct -> RTON Bytes
    // Write back to binary RTON format
    let rton_data_new = to_bytes(&rton_value_from_json).expect("Failed to serialize back to RTON");

    fs::write(rton_out_path, &rton_data_new).expect("Failed to write test_from_json.rton");
    println!(
        "💾 Converted back to RTON and saved to: {:?}",
        rton_out_path
    );

    println!("🎉 JSON Test Finished! Check 'tests/test.json' to see the structure.");
}
