use core::fmt;

use crate::template::{MAX_INPUT_BYTES, MAX_TEMPLATE_PAYLOAD_LEN};

pub const IX_CREATE_TEMPLATE: u8 = 0;
pub const IX_BEGIN_TEMPLATE: u8 = 1;
pub const IX_WRITE_TEMPLATE_CHUNK: u8 = 2;
pub const IX_FINALIZE_TEMPLATE: u8 = 3;
pub const IX_CANCEL_TEMPLATE: u8 = 4;
pub const IX_RUN: u8 = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BallistaInstruction<'data> {
    CreateTemplate {
        template_id: u16,
        payload_hash: &'data [u8; 32],
        payload: &'data [u8],
    },
    BeginTemplate {
        template_id: u16,
        payload_len: usize,
        payload_hash: &'data [u8; 32],
    },
    WriteTemplateChunk {
        offset: usize,
        bytes: &'data [u8],
    },
    FinalizeTemplate,
    CancelTemplate,
    Run {
        input_bytes: &'data [u8],
    },
}

impl<'data> BallistaInstruction<'data> {
    pub fn parse(data: &'data [u8]) -> Result<Self, InstructionError> {
        let (&discriminator, remaining) = data.split_first().ok_or(InstructionError::Truncated)?;
        match discriminator {
            IX_CREATE_TEMPLATE => {
                let (template_id, remaining) = read_u16(remaining)?;
                let (hash, payload) = read_array::<32>(remaining)?;
                if payload.is_empty() || payload.len() > MAX_TEMPLATE_PAYLOAD_LEN {
                    return Err(InstructionError::InvalidLength(payload.len()));
                }
                Ok(Self::CreateTemplate {
                    template_id,
                    payload_hash: hash,
                    payload,
                })
            }
            IX_BEGIN_TEMPLATE => {
                let (template_id, remaining) = read_u16(remaining)?;
                let (payload_len, remaining) = read_u32(remaining)?;
                let (hash, trailing) = read_array::<32>(remaining)?;
                if !trailing.is_empty() {
                    return Err(InstructionError::TrailingBytes);
                }
                let payload_len = payload_len as usize;
                if payload_len == 0 || payload_len > MAX_TEMPLATE_PAYLOAD_LEN {
                    return Err(InstructionError::InvalidLength(payload_len));
                }
                Ok(Self::BeginTemplate {
                    template_id,
                    payload_len,
                    payload_hash: hash,
                })
            }
            IX_WRITE_TEMPLATE_CHUNK => {
                let (offset, bytes) = read_u32(remaining)?;
                if bytes.is_empty() || bytes.len() > MAX_TEMPLATE_PAYLOAD_LEN {
                    return Err(InstructionError::InvalidLength(bytes.len()));
                }
                Ok(Self::WriteTemplateChunk {
                    offset: offset as usize,
                    bytes,
                })
            }
            IX_FINALIZE_TEMPLATE => {
                require_empty(remaining)?;
                Ok(Self::FinalizeTemplate)
            }
            IX_CANCEL_TEMPLATE => {
                require_empty(remaining)?;
                Ok(Self::CancelTemplate)
            }
            IX_RUN => {
                if remaining.len() > MAX_INPUT_BYTES {
                    return Err(InstructionError::InvalidLength(remaining.len()));
                }
                Ok(Self::Run {
                    input_bytes: remaining,
                })
            }
            _ => Err(InstructionError::UnknownDiscriminator(discriminator)),
        }
    }
}

fn read_u16(data: &[u8]) -> Result<(u16, &[u8]), InstructionError> {
    let (bytes, remaining) = read_array::<2>(data)?;
    Ok((u16::from_le_bytes(*bytes), remaining))
}

fn read_u32(data: &[u8]) -> Result<(u32, &[u8]), InstructionError> {
    let (bytes, remaining) = read_array::<4>(data)?;
    Ok((u32::from_le_bytes(*bytes), remaining))
}

fn read_array<const N: usize>(data: &[u8]) -> Result<(&[u8; N], &[u8]), InstructionError> {
    let (bytes, remaining) = data
        .split_at_checked(N)
        .ok_or(InstructionError::Truncated)?;
    Ok((
        bytes.try_into().map_err(|_| InstructionError::Truncated)?,
        remaining,
    ))
}

fn require_empty(data: &[u8]) -> Result<(), InstructionError> {
    if data.is_empty() {
        Ok(())
    } else {
        Err(InstructionError::TrailingBytes)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstructionError {
    Truncated,
    TrailingBytes,
    UnknownDiscriminator(u8),
    InvalidLength(usize),
}

impl fmt::Display for InstructionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for InstructionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_borrowed_create_and_run_payloads() {
        let mut create = vec![IX_CREATE_TEMPLATE];
        create.extend_from_slice(&7u16.to_le_bytes());
        create.extend_from_slice(&[9; 32]);
        create.extend_from_slice(&[1, 2, 3]);
        assert!(matches!(
            BallistaInstruction::parse(&create),
            Ok(BallistaInstruction::CreateTemplate {
                template_id: 7,
                payload: [1, 2, 3],
                ..
            })
        ));

        assert_eq!(
            BallistaInstruction::parse(&[IX_RUN, 4, 5]),
            Ok(BallistaInstruction::Run {
                input_bytes: &[4, 5]
            })
        );
    }
}
