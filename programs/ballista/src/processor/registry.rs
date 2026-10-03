//! Registry entries: accounts Ballista owns, one per template, registry and key, that only runs
//! of their template can write.
//!
//! An entry is a 72-byte header, then the registry's fields, zeroed when it is created:
//!
//! | Bytes | Holds |
//! | --- | --- |
//! | 0..4 | `BREG` |
//! | 4 | version, 1 |
//! | 5 | the registry index |
//! | 6..8 | zero |
//! | 8..40 | the template's address |
//! | 40..72 | the key |
//!
//! Its address is `find_program_address(["registry", template, [index], key], ballista)`. Only
//! [`create_entry`] writes a header, and only at that address, and only Ballista can write the data
//! of an account it owns. So a Ballista-owned account whose header names this template, registry
//! and key is that entry, and an open checks the header instead of deriving the address again.
use ballista_common::template::{
    OP_READ_BOOL, OP_READ_I64, OP_READ_PUBKEY, OP_READ_U128, OP_READ_U64,
    REGISTRY_ENTRY_HEADER_LEN, REGISTRY_ENTRY_MAGIC, REGISTRY_ENTRY_VERSION, REGISTRY_SEED,
};
use pinocchio::{
    cpi::{Seed, Signer},
    sysvars::{rent::Rent, Sysvar},
    AccountView, Address,
};
use pinocchio_system::instructions::{Allocate, Assign, CreateAccount, Transfer};

use super::execute::{RunResult, RuntimeValue};
use crate::{error::BallistaError, utils::pda::get_registry_address};

/// What names an entry: the running template, the registry index and the key.
pub struct EntryId<'a> {
    pub template: &'a Address,
    pub index: u8,
    pub key: [u8; 32],
}

impl EntryId<'_> {
    /// The header this entry holds.
    pub fn header(&self) -> [u8; REGISTRY_ENTRY_HEADER_LEN] {
        let mut header = [0; REGISTRY_ENTRY_HEADER_LEN];
        header[..4].copy_from_slice(&REGISTRY_ENTRY_MAGIC);
        header[4] = REGISTRY_ENTRY_VERSION;
        header[5] = self.index;
        header[8..40].copy_from_slice(self.template.as_ref());
        header[40..].copy_from_slice(&self.key);
        header
    }
}

/// Checks the entry `id` names in `entry`, or creates it with `payer`'s lamports, then marks the
/// entry's data exclusively borrowed for the rest of the run. An entry this run has open already
/// fails with `InvalidRegistryEntry`.
///
/// The mark is Ballista's record that the entry is open. `READ_REGISTRY` and `WRITE_REGISTRY`
/// require it, and every CPI meets it: the invocation refuses a writable account whose data is
/// borrowed, so no CPI can pass an open entry writable, and without that no nested Ballista run can
/// write the entry between this run's read and its write. The runtime refuses every other way back
/// into Ballista: a CPI chain may not re-enter a program deeper in the stack. The mark is never
/// cleared. It is a byte of the account's input region that the runtime does not read back.
#[inline(never)]
pub fn open(entry: &AccountView, payer: &AccountView, id: &EntryId, size: usize) -> RunResult<()> {
    if !entry.is_writable() {
        return Err(BallistaError::InvalidRegistryEntry.into());
    }
    if entry.owned_by(&crate::ID) {
        check_entry(entry, id, size)?;
    } else {
        create_entry(payer, entry, id, size)?;
    }
    // The verifier lets a template open each entry account once, so an account already marked here
    // is one the transaction passed in two slots: two entries of one registry whose keys came out
    // equal. The two would alias, and a template that read both and then wrote both would lose its
    // first write, so the borrow fails the run.
    let mut view = *entry;
    let borrowed = view
        .try_borrow_mut()
        .map_err(|_| BallistaError::InvalidRegistryEntry)?;
    core::mem::forget(borrowed);
    Ok(())
}

/// Checks an entry that exists: its size, and its header against `id`'s.
fn check_entry(entry: &AccountView, id: &EntryId, size: usize) -> RunResult<()> {
    // SAFETY: no reference into the entry's data outlives the opcode that took it, so none is
    // held while this one is.
    let data = unsafe { entry.borrow_unchecked() };
    if data.len() != REGISTRY_ENTRY_HEADER_LEN + size
        || data[..REGISTRY_ENTRY_HEADER_LEN] != id.header()
    {
        return Err(BallistaError::InvalidRegistryEntry.into());
    }
    Ok(())
}

/// Creates the entry `id` names in `entry`, paid for by `payer`. The only place a run signs.
///
/// The account must be the entry's canonical address, owned by the System program and empty. With
/// no lamports there, one `create_account` makes it rent-exempt and Ballista's. With lamports
/// already there, which anyone can send to block `create_account`, a transfer tops it up to rent
/// exemption and `allocate` and `assign` do the rest, as `create_template_account` does for a
/// template's address. The entry's seeds sign `create_account`, `allocate` and `assign`; the
/// transfer needs only the payer, whose signature the transaction carries. A template's own CPIs
/// go through `bounded_invoke`, which passes no seeds, so a template never holds a signature.
#[cold]
#[inline(never)]
fn create_entry(payer: &AccountView, entry: &AccountView, id: &EntryId, size: usize) -> RunResult<()> {
    if !entry.owned_by(&pinocchio_system::ID) || !entry.is_data_empty() {
        return Err(BallistaError::InvalidRegistryEntry.into());
    }
    let (address, bump) = get_registry_address(id.template, id.index, &id.key)
        .ok_or(BallistaError::InvalidRegistryEntry)?;
    if entry.address() != &address {
        return Err(BallistaError::InvalidRegistryEntry.into());
    }
    let index = [id.index];
    let bump = [bump];
    let seeds = [
        Seed::from(REGISTRY_SEED),
        Seed::from(id.template.as_ref()),
        Seed::from(index.as_ref()),
        Seed::from(id.key.as_ref()),
        Seed::from(bump.as_ref()),
    ];
    let signer = Signer::from(seeds.as_slice());
    let space = REGISTRY_ENTRY_HEADER_LEN + size;
    if entry.lamports() == 0 {
        CreateAccount::with_minimum_balance(payer, entry, space as u64, &crate::ID, None)?
            .invoke_signed(&[signer])?;
    } else {
        let required = Rent::get()?.try_minimum_balance(space)?;
        let shortfall = required.saturating_sub(entry.lamports());
        if shortfall > 0 {
            Transfer { from: payer, to: entry, lamports: shortfall }.invoke()?;
        }
        Allocate { account: entry, space: space as u64 }.invoke_signed(core::slice::from_ref(&signer))?;
        Assign { account: entry, owner: &crate::ID }.invoke_signed(&[signer])?;
    }
    let mut view = *entry;
    // SAFETY: the account is Ballista's now, `space` bytes long, and nothing holds a reference
    // into its data.
    let data = unsafe { view.borrow_unchecked_mut() };
    data.get_mut(..REGISTRY_ENTRY_HEADER_LEN)
        .ok_or(BallistaError::InvalidRegistryEntry)?
        .copy_from_slice(&id.header());
    Ok(())
}

/// Writes `value` into `entry`'s data at `offset`, which counts from the start of the data, with
/// `selector`'s width. The verifier bounds the field and the value's type; these checks keep a
/// template that got past it from writing outside the entry.
#[inline(always)]
pub fn write_field(
    entry: &AccountView,
    offset: usize,
    selector: u8,
    value: &RuntimeValue<'_>,
) -> RunResult<()> {
    let mut view = *entry;
    // SAFETY: nothing holds a reference into the entry's data, and this one ends with the write.
    let data = unsafe { view.borrow_unchecked_mut() };
    match (selector, value) {
        (OP_READ_U64, RuntimeValue::U64(value)) => put(data, offset, &value.to_le_bytes()),
        (OP_READ_I64, RuntimeValue::I64(value)) => put(data, offset, &value.to_le_bytes()),
        (OP_READ_U128, RuntimeValue::U128(value)) => put(data, offset, value),
        (OP_READ_PUBKEY, RuntimeValue::Pubkey(value)) => put(data, offset, value),
        (OP_READ_BOOL, RuntimeValue::Bool(value)) => put(data, offset, &[u8::from(*value)]),
        (_, RuntimeValue::Unset) => Err(BallistaError::InvalidRegister.into()),
        _ => Err(BallistaError::TypeMismatch.into()),
    }
}

/// Copies `bytes` to `data[offset..offset + N]`, whose length is known, so the copy is a few
/// stores rather than a `sol_memcpy_` call.
fn put<const N: usize>(data: &mut [u8], offset: usize, bytes: &[u8; N]) -> RunResult<()> {
    let field = data
        .get_mut(offset..)
        .and_then(|rest| rest.first_chunk_mut::<N>())
        .ok_or(BallistaError::InvalidTemplateProgram)?;
    *field = *bytes;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processor::execute::{execute_instruction, RunError, Scratch};
    use ballista_common::template::{
        record, InstructionRecord, ProgramBuilder, ProgramView, RegistryField, ACCOUNT_WRITABLE,
        NO_INDEX, OP_READ_REGISTRY, OP_READ_U8, OP_WRITE_REGISTRY, SYSTEM_PROGRAM_ADDRESS,
    };
    use pinocchio::account::{RuntimeAccount, NOT_BORROWED};
    use RuntimeValue::{Bool, Pubkey, Unset, I64, U128, U64};

    const TEMPLATE: [u8; 32] = [7; 32];
    const KEY: [u8; 32] = [9; 32];

    fn err(kind: BallistaError) -> RunError {
        RunError::Vm(kind)
    }

    /// An account laid out as the entrypoint hands it over: the runtime header, then its data.
    struct TestAccount {
        buffer: Vec<u64>,
    }

    impl TestAccount {
        fn new(address: [u8; 32], owner: [u8; 32], lamports: u64, writable: bool, data: &[u8]) -> Self {
            let header = core::mem::size_of::<RuntimeAccount>();
            let mut buffer = vec![0u64; (header + data.len()).div_ceil(8)];
            let account = RuntimeAccount {
                borrow_state: NOT_BORROWED,
                is_signer: 0,
                is_writable: u8::from(writable),
                executable: 0,
                padding: [0; 4],
                address: Address::new_from_array(address),
                owner: Address::new_from_array(owner),
                lamports,
                data_len: data.len() as u64,
            };
            // SAFETY: the buffer is eight-aligned and holds the header and the data.
            unsafe {
                core::ptr::write(buffer.as_mut_ptr().cast::<RuntimeAccount>(), account);
                core::ptr::copy_nonoverlapping(
                    data.as_ptr(),
                    buffer.as_mut_ptr().cast::<u8>().add(header),
                    data.len(),
                );
            }
            Self { buffer }
        }

        fn view(&mut self) -> AccountView {
            // SAFETY: `new` wrote a runtime account header followed by its data.
            unsafe { AccountView::new_unchecked(self.buffer.as_mut_ptr().cast()) }
        }
    }

    fn id(template: &Address) -> EntryId<'_> {
        EntryId { template, index: 2, key: KEY }
    }

    /// An existing 16-byte entry of registry 2 for `KEY` in `TEMPLATE`, its data as given.
    fn entry_data(fields: &[u8; 16]) -> Vec<u8> {
        let template = Address::new_from_array(TEMPLATE);
        let mut data = id(&template).header().to_vec();
        data.extend_from_slice(fields);
        data
    }

    #[test]
    fn the_header_is_magic_version_index_zeros_template_key() {
        let template = Address::new_from_array(TEMPLATE);
        let header = id(&template).header();
        assert_eq!(&header[..8], b"BREG\x01\x02\x00\x00");
        assert_eq!(header[8..40], TEMPLATE);
        assert_eq!(header[40..], KEY);
    }

    #[test]
    fn an_existing_entry_opens_only_when_everything_matches() {
        let template = Address::new_from_array(TEMPLATE);
        let crate_id = crate::ID.to_bytes();
        let good = entry_data(&[0; 16]);
        let mut payer = TestAccount::new([1; 32], SYSTEM_PROGRAM_ADDRESS, 10, true, &[]);
        let payer = payer.view();

        let mut entry = TestAccount::new([3; 32], crate_id, 10, true, &good);
        let view = entry.view();
        assert_eq!(open(&view, &payer, &id(&template), 16), Ok(()));
        assert!(view.is_borrowed_mut(), "an open marks the entry");
        assert_eq!(
            open(&view, &payer, &id(&template), 16),
            Err(err(BallistaError::InvalidRegistryEntry)),
            "a second open of an open entry: two entries whose keys are equal"
        );

        let wrong = |name: &str, data: Vec<u8>, owner: [u8; 32], writable: bool, size: usize| {
            let mut entry = TestAccount::new([3; 32], owner, 10, writable, &data);
            assert_eq!(
                open(&entry.view(), &payer, &id(&template), size),
                Err(err(BallistaError::InvalidRegistryEntry)),
                "{name}"
            );
        };
        wrong("read-only", good.clone(), crate_id, false, 16);
        wrong("declared size differs", good.clone(), crate_id, true, 24);
        wrong("owned by another program", good.clone(), [4; 32], true, 16);
        let mut other = good.clone();
        other[0] = b'X';
        wrong("magic", other, crate_id, true, 16);
        let mut other = good.clone();
        other[4] = 2;
        wrong("version", other, crate_id, true, 16);
        let mut other = good.clone();
        other[5] = 3;
        wrong("registry index", other, crate_id, true, 16);
        let mut other = good.clone();
        other[8] ^= 1;
        wrong("another template's entry", other, crate_id, true, 16);
        let mut other = good.clone();
        other[40] ^= 1;
        wrong("another key's entry", other, crate_id, true, 16);
        wrong("System-owned with data", good.clone(), SYSTEM_PROGRAM_ADDRESS, true, 16);
    }

    #[test]
    fn a_missing_entry_must_sit_at_its_derived_address() {
        let template = Address::new_from_array(TEMPLATE);
        let mut payer = TestAccount::new([1; 32], SYSTEM_PROGRAM_ADDRESS, 10, true, &[]);
        // Any address but the PDA fails before a CPI, so this runs on the host.
        let mut entry = TestAccount::new([3; 32], SYSTEM_PROGRAM_ADDRESS, 0, true, &[]);
        assert_eq!(
            open(&entry.view(), &payer.view(), &id(&template), 16),
            Err(err(BallistaError::InvalidRegistryEntry))
        );
    }

    #[test]
    fn fields_are_written_at_their_width_and_type() {
        let mut entry = TestAccount::new([3; 32], crate::ID.to_bytes(), 10, true, &entry_data(&[0; 16]));
        let view = entry.view();
        let at = |offset: usize| REGISTRY_ENTRY_HEADER_LEN + offset;
        assert_eq!(write_field(&view, at(0), OP_READ_U64, &U64(5)), Ok(()));
        assert_eq!(write_field(&view, at(8), OP_READ_I64, &I64(-2)), Ok(()));
        assert_eq!(write_field(&view, at(15), OP_READ_BOOL, &Bool(true)), Ok(()));
        // SAFETY: nothing else borrows the test account.
        let data = unsafe { view.borrow_unchecked() };
        assert_eq!(&data[at(0)..at(8)], &5u64.to_le_bytes());
        assert_eq!(&data[at(8)..at(15)], &(-2i64).to_le_bytes()[..7]);
        assert_eq!(data[at(15)], 1);

        let mut wide_account = TestAccount::new([3; 32], crate::ID.to_bytes(), 10, true, &[0; 72 + 48]);
        let wide = wide_account.view();
        assert_eq!(write_field(&wide, at(0), OP_READ_U128, &U128([6; 16])), Ok(()));
        assert_eq!(write_field(&wide, at(16), OP_READ_PUBKEY, &Pubkey([8; 32])), Ok(()));

        assert_eq!(write_field(&view, at(0), OP_READ_U64, &I64(1)), Err(err(BallistaError::TypeMismatch)));
        assert_eq!(write_field(&view, at(0), OP_READ_U64, &Unset), Err(err(BallistaError::InvalidRegister)));
        assert_eq!(write_field(&view, at(0), OP_READ_U8, &U64(1)), Err(err(BallistaError::TypeMismatch)));
        assert_eq!(
            write_field(&view, at(9), OP_READ_U64, &U64(1)),
            Err(err(BallistaError::InvalidTemplateProgram)),
            "past the end of the data"
        );
    }

    /// Reads and writes through the executor: only an entry an open has marked, and each read
    /// typed as its selector.
    #[test]
    fn reads_and_writes_go_through_the_executor_on_an_open_entry() {
        let mut builder = ProgramBuilder::new();
        let entry_account = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let value = builder.const_u64(41);
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        let mut entry = TestAccount::new([3; 32], crate::ID.to_bytes(), 10, true, &entry_data(&[0; 16]));
        let accounts = [entry.view()];
        let mut scratch = Scratch::new(&program);
        let mut registers = vec![U64(41), Unset];
        let write = builder_record(OP_WRITE_REGISTRY, NO_INDEX, value, entry_account, 0, OP_READ_U64);
        let read = builder_record(OP_READ_REGISTRY, 1, entry_account, NO_INDEX, 0, OP_READ_U64);
        let read_byte = builder_record(OP_READ_REGISTRY, 1, entry_account, NO_INDEX, 1, OP_READ_U8);

        assert_eq!(
            execute_instruction(&program, &[], &accounts, &mut registers, &mut scratch, &write, None),
            Err(err(BallistaError::InvalidTemplateProgram)),
            "not opened in this run"
        );
        let template = Address::new_from_array(TEMPLATE);
        let mut payer = TestAccount::new([1; 32], SYSTEM_PROGRAM_ADDRESS, 10, true, &[]);
        open(&accounts[0], &payer.view(), &id(&template), 16).unwrap();
        for record in [write, read] {
            assert_eq!(
                execute_instruction(&program, &[], &accounts, &mut registers, &mut scratch, &record, None),
                Ok(())
            );
        }
        assert_eq!(registers[1], U64(41));
        assert_eq!(
            execute_instruction(&program, &[], &accounts, &mut registers, &mut scratch, &read_byte, None),
            Ok(())
        );
        assert_eq!(registers[1], U64(0), "the u64's second byte");
    }

    fn builder_record(opcode: u8, dst: u8, a: u8, b: u8, offset: u16, selector: u8) -> InstructionRecord {
        let field = RegistryField { offset, selector };
        record(opcode, dst, a, b, NO_INDEX, 0, field.encode())
    }
}
