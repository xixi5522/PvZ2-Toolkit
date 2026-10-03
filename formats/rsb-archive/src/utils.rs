use std::io::{self, Read, Seek, SeekFrom};

pub(crate) trait BinReadExt: Read {
    fn read_null_term_string(&mut self) -> io::Result<String> {
        let mut bytes = Vec::new();
        loop {
            let mut byte = [0_u8; 1];
            self.read_exact(&mut byte)?;
            if byte[0] == 0 {
                break;
            }
            bytes.push(byte[0]);
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

impl<R: Read + ?Sized> BinReadExt for R {}

pub(crate) fn read_string_at<R: Read + Seek>(reader: &mut R, offset: u64) -> io::Result<String> {
    let current = reader.stream_position()?;
    reader.seek(SeekFrom::Start(offset))?;
    let value = reader.read_null_term_string();
    reader.seek(SeekFrom::Start(current))?;
    value
}
