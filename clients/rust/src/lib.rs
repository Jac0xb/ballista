use ballista_common::instruction::*;
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey,
    pubkey::Pubkey,
};

pub use ballista_common;

pub const ID: Pubkey = pubkey!("BLSTAxxzuLZzFQpwDGMMXERLCGw36u3Au3XeZNyRHpe2");
pub const BALLISTA_ID: Pubkey = ID;
pub const SYSTEM_PROGRAM_ID: Pubkey = pubkey!("11111111111111111111111111111111");
pub const TEMPLATE_SEED: &[u8] = b"template-v2";

pub fn find_template_pda(creator: &Pubkey, template_id: u16) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[TEMPLATE_SEED, creator.as_ref(), &template_id.to_le_bytes()],
        &ID,
    )
}

pub fn template_hash(payload: &[u8]) -> [u8; 32] {
    solana_sha256_hasher::hash(payload).to_bytes()
}

pub fn create_template_instruction(
    creator: Pubkey,
    template_id: u16,
    payload: &[u8],
) -> Instruction {
    let (template, _) = find_template_pda(&creator, template_id);
    let mut data = Vec::with_capacity(35 + payload.len());
    data.push(IX_CREATE_TEMPLATE);
    data.extend_from_slice(&template_id.to_le_bytes());
    data.extend_from_slice(&template_hash(payload));
    data.extend_from_slice(payload);
    Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new(creator, true),
            AccountMeta::new(template, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data,
    }
}

pub fn begin_template_instruction(
    creator: Pubkey,
    template_id: u16,
    payload_len: u32,
    payload_hash: [u8; 32],
) -> Instruction {
    let (template, _) = find_template_pda(&creator, template_id);
    let mut data = Vec::with_capacity(39);
    data.push(IX_BEGIN_TEMPLATE);
    data.extend_from_slice(&template_id.to_le_bytes());
    data.extend_from_slice(&payload_len.to_le_bytes());
    data.extend_from_slice(&payload_hash);
    Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new(creator, true),
            AccountMeta::new(template, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data,
    }
}

pub fn write_template_chunk_instruction(
    creator: Pubkey,
    template: Pubkey,
    offset: u32,
    bytes: &[u8],
) -> Instruction {
    let mut data = Vec::with_capacity(5 + bytes.len());
    data.push(IX_WRITE_TEMPLATE_CHUNK);
    data.extend_from_slice(&offset.to_le_bytes());
    data.extend_from_slice(bytes);
    Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new(creator, true),
            AccountMeta::new(template, false),
        ],
        data,
    }
}

pub fn finalize_template_instruction(creator: Pubkey, template: Pubkey) -> Instruction {
    Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new(creator, true),
            AccountMeta::new(template, false),
        ],
        data: vec![IX_FINALIZE_TEMPLATE],
    }
}

pub fn cancel_template_instruction(creator: Pubkey, template: Pubkey) -> Instruction {
    Instruction {
        program_id: ID,
        accounts: vec![
            AccountMeta::new(creator, true),
            AccountMeta::new(template, false),
        ],
        data: vec![IX_CANCEL_TEMPLATE],
    }
}

pub fn run_instruction(
    template: Pubkey,
    runtime_accounts: Vec<AccountMeta>,
    input_bytes: &[u8],
) -> Instruction {
    let mut data = Vec::with_capacity(1 + input_bytes.len());
    data.push(IX_RUN);
    data.extend_from_slice(input_bytes);
    let mut accounts = Vec::with_capacity(1 + runtime_accounts.len());
    accounts.push(AccountMeta::new_readonly(template, false));
    accounts.extend(runtime_accounts);
    Instruction {
        program_id: ID,
        accounts,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruction_codecs_use_the_v2_discriminators() {
        let creator = Pubkey::new_unique();
        let begin = begin_template_instruction(creator, 9, 10, [7; 32]);
        assert_eq!(begin.data[0], IX_BEGIN_TEMPLATE);
        assert_eq!(&begin.data[1..3], &9u16.to_le_bytes());

        let (template, _) = find_template_pda(&creator, 9);
        let run = run_instruction(template, vec![], &[1, 2]);
        assert_eq!(run.data, vec![IX_RUN, 1, 2]);
        assert_eq!(run.accounts[0].pubkey, template);
    }
}
