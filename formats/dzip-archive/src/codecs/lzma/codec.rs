use alloc::vec::Vec;

use crate::codecs::lzma::options::{encode_workspace, model_workspace};
use crate::codecs::lzma::{DecoderOptions, EncoderOptions, Error};

#[derive(Debug)]
pub struct Encoder {
    options: EncoderOptions,
    output: Vec<u8>,
}

impl Encoder {
    pub fn new(mut options: EncoderOptions) -> Result<Self, Error> {
        options.properties = options.properties.validate()?;
        Ok(Self {
            options,
            output: Vec::new(),
        })
    }

    pub fn encode<'a>(&'a mut self, input: &[u8]) -> Result<&'a [u8], Error> {
        check_input(input.len(), self.options.limits.max_input_size)?;
        let workspace = encode_workspace(input.len(), self.options.properties)
            .ok_or_else(|| Error::workspace_limit("LZMA workspace estimate overflow"))?;
        check_workspace(workspace, self.options.limits.max_workspace_size)?;
        let output = core::mem::take(&mut self.output);
        self.output = crate::codecs::lzma::engine::encode_checked_with_buffer(
            input,
            &self.options.properties,
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

    pub fn with_output(mut options: DecoderOptions, mut output: Vec<u8>) -> Result<Self, Error> {
        options.properties = options.properties.validate()?;
        if options.expected_size > options.limits.max_output_size {
            return Err(Error::output_limit(
                "LZMA declared output exceeds configured limit",
            ));
        }
        let workspace = model_workspace(options.properties)
            .ok_or_else(|| Error::workspace_limit("LZMA workspace estimate overflow"))?;
        check_workspace(workspace, options.limits.max_workspace_size)?;
        output.clear();
        Ok(Self { options, output })
    }

    pub fn decode<'a>(&'a mut self, input: &[u8]) -> Result<&'a [u8], Error> {
        check_input(input.len(), self.options.limits.max_input_size)?;
        let properties = self.options.properties.decoder_properties()?;
        let output = core::mem::take(&mut self.output);
        self.output = crate::codecs::lzma::engine::decode_raw_with_buffer(
            input,
            &properties,
            self.options.expected_size,
            output,
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
        return Err(Error::input_limit("LZMA input exceeds configured limit"));
    }
    Ok(())
}

fn check_output(actual: usize, limit: usize) -> Result<(), Error> {
    if actual > limit {
        return Err(Error::output_limit("LZMA output exceeds configured limit"));
    }
    Ok(())
}

fn check_workspace(actual: usize, limit: usize) -> Result<(), Error> {
    if actual > limit {
        return Err(Error::workspace_limit(
            "LZMA workspace exceeds configured limit",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codecs::lzma::{ErrorKind, LzmaProps, ResourceLimits};

    #[test]
    fn reusable_api_and_limits() {
        let input = b"lzma facade".repeat(200);
        let mut encoder = Encoder::new(EncoderOptions::default()).unwrap();
        let encoded = encoder.encode(&input).unwrap().to_vec();
        let mut decoder =
            Decoder::new(DecoderOptions::new(LzmaProps::default(), input.len()).unwrap()).unwrap();
        assert_eq!(decoder.decode(&encoded).unwrap(), input);

        let error = Encoder::new(EncoderOptions {
            properties: LzmaProps {
                lc: 9,
                ..LzmaProps::default()
            },
            ..EncoderOptions::default()
        })
        .err()
        .unwrap();
        assert_eq!(error.kind(), ErrorKind::InvalidOptions);

        let error = Decoder::new(DecoderOptions {
            limits: ResourceLimits {
                max_output_size: 1,
                ..ResourceLimits::UNLIMITED
            },
            ..DecoderOptions::new(LzmaProps::default(), input.len()).unwrap()
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
