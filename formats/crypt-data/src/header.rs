use crate::{CryptDataError, HEADER_SIZE, MAGIC, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Header {
    pub raw_size: usize,
}

impl Header {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < MAGIC.len() {
            return Err(CryptDataError::HeaderTooShort {
                expected: HEADER_SIZE,
            });
        }
        if !data.starts_with(&MAGIC) {
            return Err(CryptDataError::InvalidMagic);
        }
        if data.len() < HEADER_SIZE {
            return Err(CryptDataError::HeaderTooShort {
                expected: HEADER_SIZE,
            });
        }

        let raw_size = u64::from_le_bytes(
            data[MAGIC.len()..HEADER_SIZE]
                .try_into()
                .expect("CRYPT_RES size field has a fixed width"),
        );
        let raw_size = usize::try_from(raw_size).map_err(|_| CryptDataError::RawSizeOverflow)?;
        Ok(Self { raw_size })
    }

    pub fn write(self, output: &mut Vec<u8>) {
        output.extend_from_slice(&MAGIC);
        output.extend_from_slice(&(self.raw_size as u64).to_le_bytes());
    }
}
