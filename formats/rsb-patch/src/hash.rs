use md5::{Digest, Md5};

use crate::{PatchError, Result};

pub fn md5_hash(data: &[u8]) -> [u8; 16] {
    Md5::digest(data).into()
}

pub fn verify_hash(context: impl Into<String>, data: &[u8], expected: [u8; 16]) -> Result<()> {
    let actual = md5_hash(data);
    if actual != expected {
        return Err(PatchError::HashMismatch {
            context: context.into(),
            expected: hex(&expected),
            actual: hex(&actual),
        });
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(output, "{byte:02X}");
    }
    output
}
