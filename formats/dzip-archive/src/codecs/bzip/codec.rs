use alloc::vec::Vec;

use crate::codecs::bzip::options::encode_workspace;
use crate::codecs::bzip::{DecoderOptions, EncoderOptions, Error};

#[derive(Debug)]
pub struct Encoder {
    options: EncoderOptions,
    output: Vec<u8>,
}

impl Encoder {
    pub fn new(options: EncoderOptions) -> Result<Self, Error> {
        Ok(Self {
            options: options.validate()?,
            output: Vec::new(),
        })
    }

    pub fn encode<'a>(&'a mut self, input: &[u8]) -> Result<&'a [u8], Error> {
        check_input(input.len(), self.options.limits.max_input_size)?;
        let workspace = encode_workspace(input.len(), self.options.block_size)
            .ok_or_else(|| Error::workspace_limit("BZip2 workspace estimate overflow"))?;
        check_workspace(workspace, self.options.limits.max_workspace_size)?;
        let output = core::mem::take(&mut self.output);
        self.output = crate::codecs::bzip::engine::encode_with_buffer(
            input,
            self.options.block_size,
            output,
        )?;
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
                "BZip2 declared output exceeds configured limit",
            ));
        }
        output.clear();
        Ok(Self { options, output })
    }

    pub fn decode<'a>(&'a mut self, input: &[u8]) -> Result<&'a [u8], Error> {
        check_input(input.len(), self.options.limits.max_input_size)?;
        let output = core::mem::take(&mut self.output);
        self.output = crate::codecs::bzip::engine::decode_with_buffer(
            input,
            self.options.expected_size,
            output,
            self.options.limits.max_workspace_size,
        )?;
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
        return Err(Error::input_limit("BZip2 input exceeds configured limit"));
    }
    Ok(())
}

fn check_output(actual: usize, limit: usize) -> Result<(), Error> {
    if actual > limit {
        return Err(Error::output_limit("BZip2 output exceeds configured limit"));
    }
    Ok(())
}

fn check_workspace(actual: usize, limit: usize) -> Result<(), Error> {
    if actual > limit {
        return Err(Error::workspace_limit(
            "BZip2 workspace exceeds configured limit",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codecs::bzip::{ErrorKind, ResourceLimits};

    #[test]
    fn reusable_api_and_limits() {
        let input = b"bzip facade".repeat(200);
        let mut encoder = Encoder::new(EncoderOptions::default()).unwrap();
        let encoded = encoder.encode(&input).unwrap().to_vec();
        let mut decoder = Decoder::new(DecoderOptions::new(input.len())).unwrap();
        assert_eq!(decoder.decode(&encoded).unwrap(), input);

        let error = Encoder::new(EncoderOptions {
            block_size: 0,
            ..EncoderOptions::default()
        })
        .err()
        .unwrap();
        assert_eq!(error.kind(), ErrorKind::InvalidOptions);

        let error = Decoder::new(DecoderOptions {
            expected_size: input.len(),
            limits: ResourceLimits {
                max_output_size: 1,
                ..ResourceLimits::UNLIMITED
            },
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

        let first = encode(
            b"first",
            &EncoderOptions {
                block_size: 1,
                ..EncoderOptions::default()
            },
        )
        .unwrap();
        let second = encode(
            b"second",
            &EncoderOptions {
                block_size: 9,
                ..EncoderOptions::default()
            },
        )
        .unwrap();
        let mut concatenated = first;
        concatenated.extend_from_slice(&second);
        let mut decoder = Decoder::new(DecoderOptions {
            expected_size: 11,
            limits: ResourceLimits {
                max_workspace_size: crate::codecs::bzip::options::decode_workspace(1).unwrap(),
                ..ResourceLimits::UNLIMITED
            },
        })
        .unwrap();
        assert_eq!(
            decoder.decode(&concatenated).unwrap_err().kind(),
            ErrorKind::WorkspaceLimitExceeded
        );
    }
}
