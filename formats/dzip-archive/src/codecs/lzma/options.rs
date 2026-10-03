use crate::codecs::lzma::Error;

const MIN_DICTIONARY_SIZE: u32 = 1 << 12;

/// LZMA1 model and match-finder properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LzmaProps {
    pub lc: u32,
    pub lp: u32,
    pub pb: u32,
    /// Dictionary size in bytes; SDK-compatible values below 4 KiB are
    /// normalized to 4 KiB.
    pub dict_size: u32,
    /// Matching bytes that end the bounded search early.
    pub fb: u32,
    /// Maximum hash-chain candidates inspected at each position.
    pub mc: u32,
}

impl Default for LzmaProps {
    fn default() -> Self {
        Self {
            lc: 3,
            lp: 0,
            pb: 2,
            dict_size: 1 << 16,
            fb: 32,
            mc: 32,
        }
    }
}

impl LzmaProps {
    pub fn validate(mut self) -> Result<Self, Error> {
        if self.lc > 8 || self.lp > 4 || self.pb > 4 || self.lc + self.lp > 12 {
            return Err(Error::invalid_options("invalid LZMA lc/lp/pb properties"));
        }
        // The LZMA SDK decoder normalizes every encoded dictionary value below
        // 4 KiB, including zero, to its minimum dictionary size.
        if self.dict_size < MIN_DICTIONARY_SIZE {
            self.dict_size = MIN_DICTIONARY_SIZE;
        }
        Ok(self)
    }

    pub fn decoder_properties(self) -> Result<[u8; 5], Error> {
        let props = self.validate()?;
        let property = ((props.pb * 5 + props.lp) * 9 + props.lc) as u8;
        let mut output = [0u8; 5];
        output[0] = property;
        output[1..].copy_from_slice(&props.dict_size.to_le_bytes());
        Ok(output)
    }

    pub fn from_decoder_properties(properties: [u8; 5]) -> Result<Self, Error> {
        let property = u32::from(properties[0]);
        if property >= 9 * 5 * 5 {
            return Err(Error::invalid_options("invalid LZMA properties byte"));
        }
        let lc = property % 9;
        let rest = property / 9;
        let lp = rest % 5;
        let pb = rest / 5;
        Self {
            lc,
            lp,
            pb,
            dict_size: u32::from_le_bytes([
                properties[1],
                properties[2],
                properties[3],
                properties[4],
            ]),
            fb: 32,
            mc: 32,
        }
        .validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    pub max_input_size: usize,
    pub max_output_size: usize,
    pub max_workspace_size: usize,
}

impl ResourceLimits {
    pub const UNLIMITED: Self = Self {
        max_input_size: usize::MAX,
        max_output_size: usize::MAX,
        max_workspace_size: usize::MAX,
    };
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self::UNLIMITED
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EncoderOptions {
    pub properties: LzmaProps,
    pub limits: ResourceLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecoderOptions {
    pub properties: LzmaProps,
    pub expected_size: usize,
    pub limits: ResourceLimits,
}

impl DecoderOptions {
    pub fn new(properties: LzmaProps, expected_size: usize) -> Result<Self, Error> {
        Ok(Self {
            properties: properties.validate()?,
            expected_size,
            limits: ResourceLimits::UNLIMITED,
        })
    }

    pub fn from_decoder_properties(
        properties: [u8; 5],
        expected_size: usize,
    ) -> Result<Self, Error> {
        Self::new(
            LzmaProps::from_decoder_properties(properties)?,
            expected_size,
        )
    }
}

pub(crate) fn model_workspace(properties: LzmaProps) -> Option<usize> {
    let literal_contexts = 1usize.checked_shl(properties.lc + properties.lp)?;
    (0x300usize)
        .checked_mul(literal_contexts)?
        .checked_mul(core::mem::size_of::<u16>())?
        .checked_add(128 * 1024)
}

pub(crate) fn encode_workspace(input_size: usize, properties: LzmaProps) -> Option<usize> {
    model_workspace(properties)?
        .checked_add((1usize << 16) * core::mem::size_of::<usize>())?
        .checked_add(input_size.checked_mul(core::mem::size_of::<usize>())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_minimum_dictionary_is_applied_to_small_properties() {
        for dictionary in [0, 1, MIN_DICTIONARY_SIZE - 1, MIN_DICTIONARY_SIZE] {
            let props = LzmaProps {
                dict_size: dictionary,
                ..LzmaProps::default()
            }
            .validate()
            .unwrap();
            assert_eq!(props.dict_size, MIN_DICTIONARY_SIZE);
        }

        let props = LzmaProps::from_decoder_properties([0x5d, 0, 0, 0, 0]).unwrap();
        assert_eq!(props.dict_size, MIN_DICTIONARY_SIZE);
        assert_eq!(props.decoder_properties().unwrap(), [0x5d, 0, 0x10, 0, 0]);
    }
}
