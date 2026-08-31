use core::{fmt, mem::size_of};

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::{ProgramView, TemplateError, MAX_TEMPLATE_PAYLOAD_LEN};

pub const TEMPLATE_ACCOUNT_DISCRIMINATOR: u8 = 1;
pub const TEMPLATE_ACCOUNT_VERSION: u8 = 2;
pub const TEMPLATE_STATE_UPLOADING: u8 = 0;
pub const TEMPLATE_STATE_FINALIZED: u8 = 1;

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Eq, FromBytes, Immutable, IntoBytes, KnownLayout, PartialEq, Unaligned,
)]
pub struct TemplateAccountHeader {
    discriminator: u8,
    version: u8,
    state: u8,
    bump: u8,
    creator: [u8; 32],
    template_id_le: [u8; 2],
    reserved: [u8; 2],
    payload_len_le: [u8; 4],
    written_len_le: [u8; 4],
    payload_hash: [u8; 32],
}

pub const TEMPLATE_ACCOUNT_HEADER_LEN: usize = size_of::<TemplateAccountHeader>();

impl TemplateAccountHeader {
    pub fn new_uploading(
        creator: [u8; 32],
        template_id: u16,
        bump: u8,
        payload_len: usize,
        payload_hash: [u8; 32],
    ) -> Result<Self, TemplateAccountError> {
        if payload_len == 0 || payload_len > MAX_TEMPLATE_PAYLOAD_LEN {
            return Err(TemplateAccountError::InvalidPayloadLength(payload_len));
        }

        Ok(Self {
            discriminator: TEMPLATE_ACCOUNT_DISCRIMINATOR,
            version: TEMPLATE_ACCOUNT_VERSION,
            state: TEMPLATE_STATE_UPLOADING,
            bump,
            creator,
            template_id_le: template_id.to_le_bytes(),
            reserved: [0; 2],
            payload_len_le: (payload_len as u32).to_le_bytes(),
            written_len_le: 0u32.to_le_bytes(),
            payload_hash,
        })
    }

    pub const fn creator(&self) -> &[u8; 32] {
        &self.creator
    }

    pub const fn bump(&self) -> u8 {
        self.bump
    }

    pub fn template_id(&self) -> u16 {
        u16::from_le_bytes(self.template_id_le)
    }

    pub fn payload_len(&self) -> usize {
        u32::from_le_bytes(self.payload_len_le) as usize
    }

    pub fn written_len(&self) -> usize {
        u32::from_le_bytes(self.written_len_le) as usize
    }

    pub const fn payload_hash(&self) -> &[u8; 32] {
        &self.payload_hash
    }

    pub const fn state(&self) -> u8 {
        self.state
    }

    pub const fn is_finalized(&self) -> bool {
        self.state == TEMPLATE_STATE_FINALIZED
    }

    pub fn set_written_len(&mut self, written_len: usize) -> Result<(), TemplateAccountError> {
        if written_len > self.payload_len() {
            return Err(TemplateAccountError::InvalidWrittenLength {
                written: written_len,
                payload: self.payload_len(),
            });
        }
        self.written_len_le = (written_len as u32).to_le_bytes();
        Ok(())
    }

    pub fn finalize(&mut self) -> Result<(), TemplateAccountError> {
        if self.written_len() != self.payload_len() {
            return Err(TemplateAccountError::InvalidWrittenLength {
                written: self.written_len(),
                payload: self.payload_len(),
            });
        }
        self.state = TEMPLATE_STATE_FINALIZED;
        Ok(())
    }

    pub fn as_bytes(&self) -> &[u8] {
        IntoBytes::as_bytes(self)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TemplateAccount<'data> {
    header: &'data TemplateAccountHeader,
    payload: &'data [u8],
}

impl<'data> TemplateAccount<'data> {
    pub fn parse(data: &'data [u8]) -> Result<Self, TemplateAccountError> {
        let (header, payload) = TemplateAccountHeader::ref_from_prefix(data)
            .map_err(|_| TemplateAccountError::AccountTooShort(data.len()))?;

        if header.discriminator != TEMPLATE_ACCOUNT_DISCRIMINATOR {
            return Err(TemplateAccountError::InvalidDiscriminator(
                header.discriminator,
            ));
        }
        if header.version != TEMPLATE_ACCOUNT_VERSION {
            return Err(TemplateAccountError::UnsupportedVersion(header.version));
        }
        if header.reserved != [0; 2] {
            return Err(TemplateAccountError::InvalidReservedBytes);
        }
        if !matches!(
            header.state,
            TEMPLATE_STATE_UPLOADING | TEMPLATE_STATE_FINALIZED
        ) {
            return Err(TemplateAccountError::InvalidState(header.state));
        }

        let payload_len = header.payload_len();
        if payload_len == 0 || payload_len > MAX_TEMPLATE_PAYLOAD_LEN {
            return Err(TemplateAccountError::InvalidPayloadLength(payload_len));
        }
        if payload.len() != payload_len {
            return Err(TemplateAccountError::PayloadLengthMismatch {
                declared: payload_len,
                actual: payload.len(),
            });
        }
        if header.written_len() > payload_len {
            return Err(TemplateAccountError::InvalidWrittenLength {
                written: header.written_len(),
                payload: payload_len,
            });
        }
        if header.is_finalized() && header.written_len() != payload_len {
            return Err(TemplateAccountError::InvalidWrittenLength {
                written: header.written_len(),
                payload: payload_len,
            });
        }

        Ok(Self { header, payload })
    }

    pub const fn header(&self) -> &'data TemplateAccountHeader {
        self.header
    }

    pub const fn payload(&self) -> &'data [u8] {
        self.payload
    }

    pub fn finalized_program(&self) -> Result<ProgramView<'data>, TemplateAccountError> {
        if !self.header.is_finalized() {
            return Err(TemplateAccountError::NotFinalized);
        }
        ProgramView::parse(self.payload).map_err(TemplateAccountError::InvalidProgram)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemplateAccountError {
    AccountTooShort(usize),
    InvalidDiscriminator(u8),
    UnsupportedVersion(u8),
    InvalidState(u8),
    InvalidReservedBytes,
    InvalidPayloadLength(usize),
    PayloadLengthMismatch { declared: usize, actual: usize },
    InvalidWrittenLength { written: usize, payload: usize },
    NotFinalized,
    InvalidProgram(TemplateError),
}

impl fmt::Display for TemplateAccountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for TemplateAccountError {}

pub fn split_template_account_mut(
    data: &mut [u8],
) -> Result<(&mut TemplateAccountHeader, &mut [u8]), TemplateAccountError> {
    let data_len = data.len();
    let (header, payload) = TemplateAccountHeader::mut_from_prefix(data)
        .map_err(|_| TemplateAccountError::AccountTooShort(data_len))?;
    if header.discriminator != TEMPLATE_ACCOUNT_DISCRIMINATOR
        || header.version != TEMPLATE_ACCOUNT_VERSION
    {
        return Err(TemplateAccountError::InvalidDiscriminator(
            header.discriminator,
        ));
    }
    if payload.len() != header.payload_len() {
        return Err(TemplateAccountError::PayloadLengthMismatch {
            declared: header.payload_len(),
            actual: payload.len(),
        });
    }
    Ok((header, payload))
}
