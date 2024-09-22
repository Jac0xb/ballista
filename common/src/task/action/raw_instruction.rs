use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Debug, BorshDeserialize, BorshSerialize, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RawInstruction {
    pub data: Vec<u8>,
    pub num_accounts: u8,
    // transforms, allow data to be transformed before being invoked
    // conditional transform
}
