use crate::error::{Error, Result};
use simple_rijndael::impls::RijndaelCbc;
use simple_rijndael::paddings::ZeroPadding;

/// The standard encryption seed used by PvZ2 RTON files.
pub const DEFAULT_SEED: &str = "com_popcap_pvz2_magento_product_2013_05_05";

/// Derive Key and IV from a seed string using MD5.
///
/// Returns (Key, IV).
pub fn derive_key_iv(seed: &str) -> (Vec<u8>, Vec<u8>) {
    let digest = md5::compute(seed).0; // [u8; 16]
    let hex_string = hex::encode(digest); // String (32 chars)
    let hex_bytes = hex_string.as_bytes(); // &[u8] (32 bytes)

    let key = hex_bytes.to_vec(); // 32 bytes (256 bits)
    let iv = hex_bytes[4..28].to_vec(); // 24 bytes (192 bits)
    (key, iv)
}

/// Encrypt data using RTON encryption scheme (Rijndael-192-CBC with ZeroPadding).
pub fn encrypt_data(data: &[u8], seed: Option<&str>) -> Result<Vec<u8>> {
    let actual_seed = seed.unwrap_or(DEFAULT_SEED);
    let (key, iv) = derive_key_iv(actual_seed);
    let block_size = 24;

    let cipher = RijndaelCbc::<ZeroPadding>::new(&key, block_size)
        .map_err(|e| Error::Message(format!("Cipher init failed: {:?}", e)))?;

    let encrypted = cipher
        .encrypt(&iv, data.to_vec())
        .map_err(|e| Error::Message(format!("Encryption failed: {:?}", e)))?;

    Ok(encrypted)
}

/// Decrypt data using RTON encryption scheme (Rijndael-192-CBC with ZeroPadding).
pub fn decrypt_data(data: &[u8], seed: Option<&str>) -> Result<Vec<u8>> {
    let actual_seed = seed.unwrap_or(DEFAULT_SEED);
    let (key, iv) = derive_key_iv(actual_seed);
    let block_size = 24;

    let cipher = RijndaelCbc::<ZeroPadding>::new(&key, block_size)
        .map_err(|e| Error::Message(format!("Cipher init failed: {:?}", e)))?;

    let decrypted = cipher
        .decrypt(&iv, data.to_vec())
        .map_err(|e| Error::Message(format!("Decryption failed: {:?}", e)))?;

    Ok(decrypted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip_encryption() {
        let seed = "test_seed";
        let data = b"Hello, World!";

        let encrypted = encrypt_data(data, Some(seed)).expect("Encryption failed");
        assert_ne!(data, &encrypted[..]);

        let decrypted = decrypt_data(&encrypted, Some(seed)).expect("Decryption failed");
        // ZeroPadding might add null bytes, so we trim them for comparison if original data didn't end with nulls?
        // Actually, ZeroPadding pads with 0x00.
        // If our data "Hello, World!" (13 bytes), block size 24?
        // Wait, block size is 24?! Rijndael allows 128, 160, 192, 224, 256 bits.
        // 24 bytes = 192 bits.
        // So yes, block size 24 is valid for Rijndael (but not standard AES which is fixed 128).
        // RTON uses Rijndael with specific block sizes.

        // The input data "Hello, World!" is 13 bytes.
        // Padding will make it a multiple of 24.
        // Decrypted data will include padding.
        // We should check if decrypted starts with original data.

        // However, RTON usually relies on the structure itself or string lengths to ignore padding.
        // For raw data, the user of this API might need to handle padding stripping if they don't know the length.
        // But let's verify exact match for now, or prefix match.
        // Since `ZeroPadding` is used, it adds 0s.
        // "Hello, World!" ends with ! (0x21). Padding is 0x00.
        // We can trim trailing zeros.

        // For comparison, we just check if the decrypted data starts with the original data
        // RTON padding (ZeroPadding) adds null bytes at the end.
        let len = data.len();
        assert_eq!(&decrypted[..len], data);
    }
}
