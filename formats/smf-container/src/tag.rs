use md5::{Digest, Md5};

/// Return the uppercase MD5 digest used by PopCap `.tag.smf` sidecars.
pub fn md5_hex(data: &[u8]) -> String {
    let digest = Md5::digest(data);
    digest.iter().map(|byte| format!("{byte:02X}")).collect()
}

/// Return complete `.tag.smf` contents, including the expected CRLF ending.
pub fn tag_contents(data: &[u8]) -> String {
    format!("{}\r\n", md5_hex(data))
}
