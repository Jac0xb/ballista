use ballista_common::{
    instruction::BallistaInstruction,
    template::{
        split_template_account_mut, ProgramView, TemplateAccount, TemplateAccountHeader,
        TEMPLATE_ACCOUNT_HEADER_LEN, TEMPLATE_STATE_UPLOADING,
    },
};
use error::{verifier_error, BallistaError};
use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{rent::Rent, Sysvar},
    AccountView, Address, ProgramResult,
};
use pinocchio_system::instructions::{Allocate, Assign, CreateAccount, Transfer};
use solana_address::declare_id;
use solana_sha256_hasher::hash;
use utils::pda::{get_template_address, TEMPLATE_SEED};

pub mod error;
pub mod processor;
pub mod utils;

declare_id!("BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD");

#[cfg(not(feature = "no-entrypoint"))]
mod init {
    use crate::process_instruction;
    use pinocchio::entrypoint;
    // Template account plus up to 120 runtime accounts, with room to spare.
    entrypoint!(process_instruction, 128);
}

pub fn process_instruction(
    _program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = BallistaInstruction::parse(instruction_data)
        .map_err(|_| BallistaError::InvalidInstructionData)?;

    match instruction {
        BallistaInstruction::CreateTemplate {
            template_id,
            payload_hash,
            payload,
        } => create_template(accounts, template_id, payload_hash, payload),
        BallistaInstruction::BeginTemplate {
            template_id,
            payload_len,
            payload_hash,
        } => begin_template(accounts, template_id, payload_len, payload_hash),
        BallistaInstruction::WriteTemplateChunk { offset, bytes } => {
            write_template_chunk(accounts, offset, bytes)
        }
        BallistaInstruction::FinalizeTemplate => finalize_template(accounts),
        BallistaInstruction::CancelTemplate => cancel_template(accounts),
        BallistaInstruction::Run { input_bytes } => run_template(accounts, input_bytes),
    }
}

fn create_template(
    accounts: &mut [AccountView],
    template_id: u16,
    payload_hash: &[u8; 32],
    payload: &[u8],
) -> ProgramResult {
    let program = ProgramView::parse(payload).map_err(verifier_error)?;
    program.verify().map_err(verifier_error)?;
    if hash(payload).to_bytes() != *payload_hash {
        return Err(BallistaError::HashMismatch.into());
    }

    let [creator, template, system_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    validate_create_accounts(creator, template, system_program, template_id)?;
    let (_, bump) = get_template_address(creator.address(), template_id);
    create_template_account(
        creator,
        template,
        template_id,
        bump,
        payload.len(),
        *payload_hash,
    )?;

    let mut data = template.try_borrow_mut()?;
    let (header, stored_payload) =
        split_template_account_mut(&mut data).map_err(|_| BallistaError::InvalidTemplateAccount)?;
    stored_payload.copy_from_slice(payload);
    header
        .set_written_len(payload.len())
        .and_then(|_| header.finalize())
        .map_err(|_| BallistaError::InvalidTemplateAccount)?;
    Ok(())
}

fn begin_template(
    accounts: &mut [AccountView],
    template_id: u16,
    payload_len: usize,
    payload_hash: &[u8; 32],
) -> ProgramResult {
    let [creator, template, system_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    validate_create_accounts(creator, template, system_program, template_id)?;
    let (_, bump) = get_template_address(creator.address(), template_id);
    create_template_account(
        creator,
        template,
        template_id,
        bump,
        payload_len,
        *payload_hash,
    )
}

fn write_template_chunk(
    accounts: &mut [AccountView],
    offset: usize,
    bytes: &[u8],
) -> ProgramResult {
    let [creator, template] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    validate_owned_writable_template(creator, template)?;
    let template_address = *template.address();

    let mut data = template.try_borrow_mut()?;
    let (header, payload) =
        split_template_account_mut(&mut data).map_err(|_| BallistaError::InvalidTemplateAccount)?;
    validate_creator_and_pda(creator, &template_address, header)?;
    if header.state() != TEMPLATE_STATE_UPLOADING {
        return Err(BallistaError::TemplateNotUploading.into());
    }
    if header.written_len() != offset {
        return Err(BallistaError::InvalidChunkOffset.into());
    }
    let end = offset
        .checked_add(bytes.len())
        .filter(|end| *end <= payload.len())
        .ok_or(BallistaError::InvalidChunkOffset)?;
    payload[offset..end].copy_from_slice(bytes);
    header
        .set_written_len(end)
        .map_err(|_| BallistaError::InvalidChunkOffset)?;
    Ok(())
}

fn finalize_template(accounts: &mut [AccountView]) -> ProgramResult {
    let [creator, template] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    validate_owned_writable_template(creator, template)?;

    {
        let data = template.try_borrow()?;
        let account =
            TemplateAccount::parse(&data).map_err(|_| BallistaError::InvalidTemplateAccount)?;
        validate_creator_and_pda(creator, template.address(), account.header())?;
        if account.header().state() != TEMPLATE_STATE_UPLOADING {
            return Err(BallistaError::TemplateNotUploading.into());
        }
        if account.header().written_len() != account.header().payload_len() {
            return Err(BallistaError::InvalidChunkOffset.into());
        }
        if hash(account.payload()).to_bytes() != *account.header().payload_hash() {
            return Err(BallistaError::HashMismatch.into());
        }
        ProgramView::parse(account.payload())
            .and_then(|program| program.verify())
            .map_err(verifier_error)?;
    }

    let mut data = template.try_borrow_mut()?;
    let (header, _) =
        split_template_account_mut(&mut data).map_err(|_| BallistaError::InvalidTemplateAccount)?;
    header
        .finalize()
        .map_err(|_| BallistaError::InvalidTemplateAccount)?;
    Ok(())
}

fn cancel_template(accounts: &mut [AccountView]) -> ProgramResult {
    let [creator, template] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    validate_owned_writable_template(creator, template)?;
    {
        let data = template.try_borrow()?;
        let account =
            TemplateAccount::parse(&data).map_err(|_| BallistaError::InvalidTemplateAccount)?;
        validate_creator_and_pda(creator, template.address(), account.header())?;
        if account.header().state() != TEMPLATE_STATE_UPLOADING {
            return Err(BallistaError::TemplateNotUploading.into());
        }
    }

    let recovered = creator
        .lamports()
        .checked_add(template.lamports())
        .ok_or(ProgramError::ArithmeticOverflow)?;
    creator.set_lamports(recovered);
    template.set_lamports(0);
    template.close()
}

fn run_template(accounts: &mut [AccountView], input_bytes: &[u8]) -> ProgramResult {
    let [template, runtime_accounts @ ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    if !template.owned_by(&crate::ID) || template.is_data_empty() {
        return Err(BallistaError::InvalidTemplateAccount.into());
    }
    let data = template.try_borrow()?;
    let account =
        TemplateAccount::parse(&data).map_err(|_| BallistaError::InvalidTemplateAccount)?;
    let program = account
        .finalized_program()
        .map_err(|_| BallistaError::TemplateNotFinalized)?;
    processor::run(&program, input_bytes, runtime_accounts, template.address())
}

fn validate_create_accounts(
    creator: &AccountView,
    template: &AccountView,
    system_program: &AccountView,
    template_id: u16,
) -> ProgramResult {
    if !creator.is_signer() || !creator.is_writable() || !template.is_writable() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if system_program.address() != &pinocchio_system::ID || !system_program.executable() {
        return Err(ProgramError::IncorrectProgramId);
    }
    let (expected, _) = get_template_address(creator.address(), template_id);
    if template.address() != &expected
        || !template.owned_by(&pinocchio_system::ID)
        || !template.is_data_empty()
    {
        return Err(BallistaError::InvalidTemplateAccount.into());
    }
    Ok(())
}

fn validate_owned_writable_template(
    creator: &AccountView,
    template: &AccountView,
) -> ProgramResult {
    if !creator.is_signer() || !creator.is_writable() || !template.is_writable() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if !template.owned_by(&crate::ID) || template.is_data_empty() {
        return Err(BallistaError::InvalidTemplateAccount.into());
    }
    Ok(())
}

fn validate_creator_and_pda(
    creator: &AccountView,
    template_address: &Address,
    header: &TemplateAccountHeader,
) -> ProgramResult {
    if creator.address().as_ref() != header.creator() {
        return Err(BallistaError::InvalidCreator.into());
    }
    let (expected, bump) = get_template_address(creator.address(), header.template_id());
    if template_address != &expected || header.bump() != bump {
        return Err(BallistaError::InvalidTemplateAccount.into());
    }
    Ok(())
}

fn create_template_account(
    creator: &AccountView,
    template: &mut AccountView,
    template_id: u16,
    bump: u8,
    payload_len: usize,
    payload_hash: [u8; 32],
) -> ProgramResult {
    let header = TemplateAccountHeader::new_uploading(
        creator.address().to_bytes(),
        template_id,
        bump,
        payload_len,
        payload_hash,
    )
    .map_err(|_| BallistaError::InvalidTemplateProgram)?;
    let id_bytes = template_id.to_le_bytes();
    let bump_bytes = [bump];
    let seeds = [
        Seed::from(TEMPLATE_SEED),
        Seed::from(creator.address().as_ref()),
        Seed::from(id_bytes.as_ref()),
        Seed::from(bump_bytes.as_ref()),
    ];
    let signer = Signer::from(seeds.as_slice());
    let space = TEMPLATE_ACCOUNT_HEADER_LEN + payload_len;
    if template.lamports() == 0 {
        CreateAccount::with_minimum_balance(creator, template, space as u64, &crate::ID, None)?
            .invoke_signed(&[signer])?;
    } else {
        // Someone already sent lamports to this address. `CreateAccount` would reject it, which
        // lets anyone block a template ID with dust. Top up to rent exemption if needed, then
        // allocate and assign under the PDA signature exactly as the ATA program does.
        let required = Rent::get()?.try_minimum_balance(space)?;
        let shortfall = required.saturating_sub(template.lamports());
        if shortfall > 0 {
            Transfer {
                from: creator,
                to: template,
                lamports: shortfall,
            }
            .invoke()?;
        }
        Allocate {
            account: template,
            space: space as u64,
        }
        .invoke_signed(&[signer.clone()])?;
        Assign {
            account: template,
            owner: &crate::ID,
        }
        .invoke_signed(&[signer])?;
    }

    let mut data = template.try_borrow_mut()?;
    data[..TEMPLATE_ACCOUNT_HEADER_LEN].copy_from_slice(header.as_bytes());
    Ok(())
}
