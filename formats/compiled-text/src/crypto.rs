use md5::{Digest, Md5};
use simple_rijndael::{impls::RijndaelCbc, paddings::ZeroPadding};

use crate::{CompiledTextError, RIJNDAEL_BLOCK_SIZE, Result};

fn derive_key_and_iv(seed: &str) -> ([u8; 32], [u8; RIJNDAEL_BLOCK_SIZE]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest: [u8; 16] = Md5::digest(seed.as_bytes()).into();
    let mut key = [0_u8; 32];
    for (index, byte) in digest.into_iter().enumerate() {
        key[index * 2] = HEX[usize::from(byte >> 4)];
        key[index * 2 + 1] = HEX[usize::from(byte & 0x0f)];
    }
    let mut iv = [0_u8; RIJNDAEL_BLOCK_SIZE];
    iv.copy_from_slice(&key[4..28]);
    (key, iv)
}

pub(crate) fn encrypt(data: &[u8], seed: &str) -> Result<Vec<u8>> {
    let (key, iv) = derive_key_and_iv(seed);
    let cipher = RijndaelCbc::<ZeroPadding>::new(&key, RIJNDAEL_BLOCK_SIZE)
        .map_err(|error| CompiledTextError::Cipher(format!("{error:?}")))?;
    cipher
        .encrypt(&iv, data.to_vec())
        .map_err(|error| CompiledTextError::Cipher(format!("{error:?}")))
}

pub(crate) fn decrypt(data: &[u8], seed: &str) -> Result<Vec<u8>> {
    if data.is_empty() {
        return Err(CompiledTextError::EmptyCiphertext);
    }
    if !data.len().is_multiple_of(RIJNDAEL_BLOCK_SIZE) {
        return Err(CompiledTextError::CiphertextAlignment {
            actual: data.len(),
            block_size: RIJNDAEL_BLOCK_SIZE,
        });
    }
    let (key, iv) = derive_key_and_iv(seed);
    let cipher = RijndaelCbc::<ZeroPadding>::new(&key, RIJNDAEL_BLOCK_SIZE)
        .map_err(|error| CompiledTextError::Cipher(format!("{error:?}")))?;
    cipher
        .decrypt(&iv, data.to_vec())
        .map_err(|error| CompiledTextError::Cipher(format!("{error:?}")))
}
