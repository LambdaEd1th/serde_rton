use serde_rton::{RtonValue, from_bytes, to_bytes};
use std::fs;
use std::path::Path;

#[test]
fn test_rton_to_yaml_and_back() {
    // 1. Setup file paths
    let input_path = Path::new("tests/test.rton");
    let yaml_out_path = Path::new("tests/test.yaml");
    let rton_out_path = Path::new("tests/test_from_yaml.rton");

    // 2. Validate input existence
    if !input_path.exists() {
        eprintln!("⚠️ Warning: 'tests/test.rton' not found. Skipping integration test.");
        return;
    }

    println!("📂 Reading RTON from: {:?}", input_path);
    let rton_data = fs::read(input_path).expect("Failed to read test.rton");

    // 3. RTON -> Rust Struct (RtonValue)
    let rton_value: RtonValue = from_bytes(&rton_data).expect("Failed to deserialize RTON");
    println!("✅ RTON deserialized successfully.");

    // 4. Rust Struct -> YAML String
    let yaml_string = serde_yaml::to_string(&rton_value).expect("Failed to serialize to YAML");

    fs::write(yaml_out_path, &yaml_string).expect("Failed to write test.yaml");
    println!("📝 Converted to YAML and saved to: {:?}", yaml_out_path);

    // 5. YAML String -> Rust Struct (RtonValue)
    let rton_value_from_yaml: RtonValue =
        serde_yaml::from_str(&yaml_string).expect("Failed to deserialize YAML");

    // 6. Rust Struct -> RTON Bytes
    let rton_data_new = to_bytes(&rton_value_from_yaml).expect("Failed to serialize back to RTON");

    fs::write(rton_out_path, &rton_data_new).expect("Failed to write test_new.rton");
    println!(
        "💾 Converted back to RTON and saved to: {:?}",
        rton_out_path
    );

    println!("🎉 Test Finished! Check 'tests/test.yaml' to see the structure.");
}
