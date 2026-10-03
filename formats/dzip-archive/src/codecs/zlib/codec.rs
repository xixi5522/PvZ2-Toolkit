use alloc::vec::Vec;

use crate::codecs::zlib::options::{decode_workspace, encode_workspace};
use crate::codecs::zlib::{DecoderOptions, EncoderOptions, Error, StreamFormat};

#[derive(Debug)]
pub struct Encoder {
    options: EncoderOptions,
    output: Vec<u8>,
}

impl Encoder {
    pub fn new(options: EncoderOptions) -> Result<Self, Error> {
        Ok(Self {
            options,
            output: Vec::new(),
        })
    }

    pub fn encode<'a>(&'a mut self, input: &[u8]) -> Result<&'a [u8], Error> {
        check_input(input.len(), self.options.limits.max_input_size)?;
        let workspace = encode_workspace(input.len())
            .ok_or_else(|| Error::workspace_limit("DEFLATE workspace estimate overflow"))?;
        check_workspace(workspace, self.options.limits.max_workspace_size)?;
        let output = core::mem::take(&mut self.output);
        self.output = match self.options.format {
            StreamFormat::RawDeflate => {
                crate::codecs::zlib::engine::encode_raw_with_buffer(input, output)
            }
            StreamFormat::Zlib => {
                crate::codecs::zlib::engine::encode_zlib_with_buffer(input, output)
            }
        };
        check_output(self.output.len(), self.options.limits.max_output_size)?;
        Ok(&self.output)
    }

    pub fn output(&self) -> &[u8] {
        &self.output
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.output)
    }
}

#[derive(Debug)]
pub struct Decoder {
    options: DecoderOptions,
    output: Vec<u8>,
}

impl Decoder {
    pub fn new(options: DecoderOptions) -> Result<Self, Error> {
        Self::with_output(options, Vec::new())
    }

    pub fn with_output(options: DecoderOptions, mut output: Vec<u8>) -> Result<Self, Error> {
        if options.expected_size > options.limits.max_output_size {
            return Err(Error::output_limit(
                "DEFLATE declared output exceeds configured limit",
            ));
        }
        check_workspace(decode_workspace(), options.limits.max_workspace_size)?;
        output.clear();
        Ok(Self { options, output })
    }

    pub fn decode<'a>(&'a mut self, input: &[u8]) -> Result<&'a [u8], Error> {
        check_input(input.len(), self.options.limits.max_input_size)?;
        let output = core::mem::take(&mut self.output);
        self.output = match self.options.format {
            StreamFormat::RawDeflate => crate::codecs::zlib::engine::decode_raw_with_buffer(
                input,
                self.options.expected_size,
                output,
            )?,
            StreamFormat::Zlib => crate::codecs::zlib::engine::decode_zlib_with_buffer(
                input,
                self.options.expected_size,
                output,
            )?,
        };
        Ok(&self.output)
    }

    pub fn output(&self) -> &[u8] {
        &self.output
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.output)
    }
}

pub fn encode(input: &[u8], options: &EncoderOptions) -> Result<Vec<u8>, Error> {
    let mut encoder = Encoder::new(*options)?;
    encoder.encode(input)?;
    Ok(encoder.take_output())
}

pub fn decode(input: &[u8], options: &DecoderOptions) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new(*options)?;
    decoder.decode(input)?;
    Ok(decoder.take_output())
}

fn check_input(actual: usize, limit: usize) -> Result<(), Error> {
    if actual > limit {
        return Err(Error::input_limit("DEFLATE input exceeds configured limit"));
    }
    Ok(())
}

fn check_output(actual: usize, limit: usize) -> Result<(), Error> {
    if actual > limit {
        return Err(Error::output_limit(
            "DEFLATE output exceeds configured limit",
        ));
    }
    Ok(())
}

fn check_workspace(actual: usize, limit: usize) -> Result<(), Error> {
    if actual > limit {
        return Err(Error::workspace_limit(
            "DEFLATE workspace exceeds configured limit",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codecs::zlib::{ErrorKind, ResourceLimits};

    #[test]
    fn formats_reusable_api_and_limits() {
        let input = b"deflate facade".repeat(200);
        for format in [StreamFormat::RawDeflate, StreamFormat::Zlib] {
            let mut encoder = Encoder::new(EncoderOptions {
                format,
                ..EncoderOptions::default()
            })
            .unwrap();
            let encoded = encoder.encode(&input).unwrap().to_vec();
            let mut decoder = Decoder::new(DecoderOptions {
                format,
                ..DecoderOptions::new(input.len())
            })
            .unwrap();
            assert_eq!(decoder.decode(&encoded).unwrap(), input);
        }

        let error = Decoder::new(DecoderOptions {
            limits: ResourceLimits {
                max_output_size: 1,
                ..ResourceLimits::UNLIMITED
            },
            ..DecoderOptions::new(input.len())
        })
        .err()
        .unwrap();
        assert_eq!(error.kind(), ErrorKind::OutputLimitExceeded);

        let mut encoder = Encoder::new(EncoderOptions {
            limits: ResourceLimits {
                max_workspace_size: 0,
                ..ResourceLimits::UNLIMITED
            },
            ..EncoderOptions::default()
        })
        .unwrap();
        assert_eq!(
            encoder.encode(&input).unwrap_err().kind(),
            ErrorKind::WorkspaceLimitExceeded
        );
    }
}
