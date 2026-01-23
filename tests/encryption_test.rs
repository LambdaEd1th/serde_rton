#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_rton::{Error, from_bytes_with_key, from_reader_with_key, to_bytes};
    use simple_rijndael::impls::RijndaelCbc;
    use simple_rijndael::paddings::ZeroPadding;
    use std::io::Cursor;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct TestData {
        id: u32,
        name: String,
    }

    // Helper to encrypt data manually for testing since we don't have an encryption function in the library yet (only decryption supports file wrapper).
    fn create_encrypted_rton(data: &[u8], key_seed: &str) -> Vec<u8> {
        let mut out = Vec::new();
        // Write 0x10 header (LE)
        out.extend_from_slice(&[0x10, 0x00]);

        let digest = md5::compute(key_seed).0;
        let hex_string = hex::encode(digest);
        let hex_bytes = hex_string.as_bytes();
        let key = hex_bytes.to_vec();
        let iv = hex_bytes[4..28].to_vec();
        let block_size = 24;

        let cipher = RijndaelCbc::<ZeroPadding>::new(&key, block_size).unwrap();
        // Need to pad data? create_encrypted_rton assumes data is raw RTON.
        // encryption function of simple-rijndael might return padded result (with ZeroPadding) but wait, does it?
        // simple-rijndael encrypt might pad.
        // If data is RTON, it might not be aligned.
        let encrypted = cipher.encrypt(&iv, data.to_vec()).unwrap();
        out.extend(encrypted);
        out
    }

    #[test]
    fn test_encrypted_deserialize() {
        let original = TestData {
            id: 12345,
            name: "Secret Data".to_string(),
        };

        let rton_bytes = to_bytes(&original).unwrap();
        let key_seed = "my_secret_key";

        let encrypted_file = create_encrypted_rton(&rton_bytes, key_seed);

        // Test from_bytes_with_key
        let deserialized: TestData = from_bytes_with_key(&encrypted_file, Some(key_seed))
            .expect("Failed to deserialize encrypted");
        assert_eq!(original, deserialized);

        // Test from_reader_with_key
        let reader = Cursor::new(&encrypted_file);
        let deserialized_reader: TestData = from_reader_with_key(reader, Some(key_seed))
            .expect("Failed to deserialize reader encrypted");
        assert_eq!(original, deserialized_reader);
    }

    #[test]
    fn test_missing_key_error() {
        let original = TestData {
            id: 1,
            name: "A".into(),
        };
        let rton_bytes = to_bytes(&original).unwrap();
        let encrypted_file = create_encrypted_rton(&rton_bytes, "key");

        let err = from_bytes_with_key::<TestData>(&encrypted_file, None);
        assert!(matches!(err, Err(Error::MissingKey)));
    }

    #[test]
    fn test_wrong_key_error() {
        let original = TestData {
            id: 1,
            name: "A".into(),
        };
        let rton_bytes = to_bytes(&original).unwrap();
        let encrypted_file = create_encrypted_rton(&rton_bytes, "key1");

        // If wrong key, decryption produces garbage.
        // Garbage will likely fail RTON header check or other deserialization steps.
        // It returns DecryptionError only if padding fails (unlikely with ZeroPadding) or io error?
        // Or if decrypted data is invalid RTON.

        // simple-rijndael decrypt with ZeroPadding might panic if input length is not multiple of block size, but here it is.
        // Decrypt produces garbage.
        // validate_header_and_decrypt calls decrypt, then presumably we recursively call from_reader.
        // The garbage data will be checked for RTON header.
        // It will fail InvalidHeader.

        let res = from_bytes_with_key::<TestData>(&encrypted_file, Some("key2"));
        assert!(res.is_err());
        // Specific error depends on what garbage looks like. Likely InvalidHeader.
    }

    #[test]
    fn test_roundtrip_encryption() {
        use serde_rton::{to_bytes_with_key, to_writer_with_key};

        let original = TestData {
            id: 999,
            name: "Roundtrip Test".to_string(),
        };
        let key_seed = "roundtrip_key";

        // Test to_bytes_with_key -> from_bytes_with_key
        let encrypted_bytes =
            to_bytes_with_key(&original, Some(key_seed)).expect("Encryption failed");

        // Check header 0x10 (u16 LE -> 10 00)
        assert_eq!(&encrypted_bytes[0..2], &[0x10, 0x00]);

        let decrypted: TestData =
            from_bytes_with_key(&encrypted_bytes, Some(key_seed)).expect("Decryption failed");
        assert_eq!(original, decrypted);

        // Test to_writer_with_key -> from_reader_with_key
        let mut buffer = Vec::new();
        to_writer_with_key(&mut buffer, &original, Some(key_seed))
            .expect("Encryption writer failed");

        assert_eq!(buffer, encrypted_bytes);

        let cursor = Cursor::new(buffer);
        let decrypted_reader: TestData =
            from_reader_with_key(cursor, Some(key_seed)).expect("Decryption reader failed");
        assert_eq!(original, decrypted_reader);
    }
}
