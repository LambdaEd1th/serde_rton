use crate::error::{Error, Result};
use md5::{Digest, Md5};
use simple_rijndael::impls::RijndaelCbc;
use simple_rijndael::paddings::ZeroPadding;

/// The standard encryption seed used by PvZ2 RTON files.
pub const DEFAULT_SEED: &str = "com_popcap_pvz2_magento_product_2013_05_05";

/// Encrypted RTON prefix — signals that the content is Rijndael-192-CBC ciphertext.
const ENCRYPTED_PREFIX: [u8; 2] = [0x10, 0x00];

/// Derive Key and IV from a seed string using MD5.
///
/// Returns (Key, IV).
pub fn derive_key_iv(seed: &str) -> (Vec<u8>, Vec<u8>) {
    let digest: [u8; 16] = Md5::digest(seed.as_bytes()).into();
    let hex_string = hex::encode(digest); // String (32 chars)
    let hex_bytes = hex_string.as_bytes(); // &[u8] (32 bytes)

    let key = hex_bytes.to_vec(); // 32 bytes (256 bits)
    let iv = hex_bytes[4..28].to_vec(); // 24 bytes (192 bits)
    (key, iv)
}

/// Encrypt data and prepend the encrypted-RTON prefix `[0x10, 0x00]`.
///
/// Output format: `ENCRYPTED_PREFIX || Rijndael-192-CBC(ciphertext)`.
pub fn encrypt_data(data: &[u8]) -> Result<Vec<u8>> {
    let (key, iv) = derive_key_iv(DEFAULT_SEED);
    let block_size = 24;

    let cipher = RijndaelCbc::<ZeroPadding>::new(&key, block_size)
        .map_err(|e| Error::Message(format!("Cipher init failed: {:?}", e)))?;

    let encrypted = cipher
        .encrypt(&iv, data.to_vec())
        .map_err(|e| Error::Message(format!("Encryption failed: {:?}", e)))?;

    let mut out = Vec::with_capacity(2 + encrypted.len());
    out.extend_from_slice(&ENCRYPTED_PREFIX);
    out.extend_from_slice(&encrypted);
    Ok(out)
}

/// Decrypt data that starts with the encrypted-RTON prefix `[0x10, 0x00]`.
///
/// Strips the prefix, then decrypts the remainder with Rijndael-192-CBC.
pub fn decrypt_data(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 2 || data[..2] != ENCRYPTED_PREFIX {
        return Err(Error::DecryptionError(
            "Input does not start with encrypted-RTON prefix [0x10, 0x00]".into(),
        ));
    }

    let (key, iv) = derive_key_iv(DEFAULT_SEED);
    let block_size = 24;

    let cipher = RijndaelCbc::<ZeroPadding>::new(&key, block_size)
        .map_err(|e| Error::Message(format!("Cipher init failed: {:?}", e)))?;

    let decrypted = cipher
        .decrypt(&iv, data[2..].to_vec())
        .map_err(|e| Error::Message(format!("Decryption failed: {:?}", e)))?;

    Ok(decrypted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip_encryption() {
        let data = b"Hello, World!";

        let encrypted = encrypt_data(data).expect("Encryption failed");
        assert!(&encrypted[..2] == b"\x10\x00", "Missing prefix");
        assert_ne!(data, &encrypted[..]);

        let decrypted = decrypt_data(&encrypted).expect("Decryption failed");

        let len = data.len();
        assert_eq!(&decrypted[..len], data);
    }

    #[test]
    fn test_decrypt_rejects_missing_prefix() {
        let result = decrypt_data(b"raw_data_without_prefix");
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_rejects_too_short() {
        let result = decrypt_data(b"\x10");
        assert!(result.is_err());
    }
}
