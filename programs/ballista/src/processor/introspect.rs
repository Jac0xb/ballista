//! Transaction introspection through the Instructions sysvar, and bounded byte reads from
//! instruction data and from read-only accounts.
//!
//! Nothing here copies bytes. A `bytes` result borrows the sysvar's data or the account's for the
//! rest of the run, which is sound only because neither can change while this instruction runs:
//! see `sysvar_data` and `read_only_data`.

use ballista_common::template::*;
use pinocchio::{
    sysvars::instructions::{Instructions, INSTRUCTIONS_ID},
    AccountView,
};

use super::execute::{get, read_value, set, RunError, RunResult, RuntimeValue};
use crate::error::BallistaError;

/// `INSTRUCTION_COUNT` or `INSTRUCTION_INDEX`: the two bytes at the front or the back of the
/// sysvar. No parsing, so the router runs these itself.
#[inline(always)]
pub fn count_or_index(opcode: u8, sysvar: &AccountView) -> RunResult<u64> {
    // SAFETY: `sysvar_data` checked this is the Instructions sysvar, whose layout this parses.
    let instructions = unsafe { Instructions::new_unchecked(sysvar_data(sysvar)?) };
    Ok(if opcode == OP_INSTRUCTION_COUNT {
        instructions.num_instructions() as u64
    } else {
        u64::from(instructions.load_current_index())
    })
}

/// `BYTES_LEN`: the length of the `bytes` value in `value`, the register operand `a`.
#[inline(always)]
pub fn bytes_len(value: &RuntimeValue<'_>) -> RunResult<u64> {
    match value {
        RuntimeValue::Bytes(bytes) => Ok(bytes.len() as u64),
        RuntimeValue::Unset => Err(BallistaError::InvalidRegister.into()),
        _ => Err(BallistaError::TypeMismatch.into()),
    }
}

/// Runs one of the opcodes that parse the Instructions sysvar, from `OP_INSTRUCTION_PROGRAM` to
/// `OP_READ_INSTRUCTION_BYTES`, against `sysvar`, the account `a` names.
#[inline(never)]
pub fn read_instruction<'data>(
    sysvar: &'data AccountView,
    registers: &mut [RuntimeValue<'data>],
    instruction: &InstructionRecord,
) -> RunResult<()> {
    let data = sysvar_data(sysvar)?;
    // SAFETY: `sysvar_data` checked the account's address, and the runtime writes that account's
    // data in exactly the layout `introspect` parses.
    let value = unsafe { introspect(data, registers, instruction) }?;
    set(registers, instruction.dst as usize, value)
}

/// Runs `READ_ACCOUNT_BYTES`: `immediate` bytes of `account`'s data from the offset in register
/// `b`.
#[inline(never)]
pub fn read_account_bytes<'data>(
    account: &'data AccountView,
    registers: &mut [RuntimeValue<'data>],
    instruction: &InstructionRecord,
) -> RunResult<()> {
    let offset = position(registers, instruction.b)?;
    let bytes = byte_range(read_only_data(account)?, offset, instruction.immediate())?;
    set(
        registers,
        instruction.dst as usize,
        RuntimeValue::Bytes(bytes),
    )
}

/// The value one sysvar opcode reads from `sysvar`, the Instructions sysvar's data. Takes the data
/// rather than the account so the host tests and formal specifications, which have no account to
/// hand it, can call it.
///
/// # Safety
///
/// `sysvar` must be the Instructions sysvar's data, in exactly the layout the runtime writes; on
/// chain, the slice `sysvar_data` returns once it has checked the account's address. pinocchio's
/// parser trusts the instruction count, the offset table and every length it finds, and reads
/// through them without bounds checks, so any other bytes can send it outside the slice.
pub unsafe fn introspect<'data>(
    sysvar: &'data [u8],
    registers: &[RuntimeValue<'data>],
    instruction: &InstructionRecord,
) -> RunResult<RuntimeValue<'data>> {
    // SAFETY: the caller guarantees `sysvar` is the Instructions sysvar's data, in the layout
    // `Instructions` parses.
    let instructions = unsafe { Instructions::new_unchecked(sysvar) };
    match instruction.opcode {
        OP_INSTRUCTION_COUNT => {
            return Ok(RuntimeValue::U64(instructions.num_instructions() as u64))
        }
        OP_INSTRUCTION_INDEX => {
            return Ok(RuntimeValue::U64(u64::from(
                instructions.load_current_index(),
            )))
        }
        _ => {}
    }
    // pinocchio checks the index and the account position; the data ranges are checked here.
    let index = position(registers, instruction.b)?;
    let introspected = instructions
        .load_instruction_at(index)
        .map_err(|_| out_of_range())?;
    Ok(match instruction.opcode {
        OP_INSTRUCTION_PROGRAM => RuntimeValue::Pubkey(introspected.get_program_id().to_bytes()),
        OP_INSTRUCTION_ACCOUNT_COUNT => RuntimeValue::U64(introspected.num_account_metas() as u64),
        OP_INSTRUCTION_ACCOUNT | OP_INSTRUCTION_ACCOUNT_FLAGS => {
            let account = introspected
                .get_instruction_account_at(position(registers, instruction.c)?)
                .map_err(|_| out_of_range())?;
            if instruction.opcode == OP_INSTRUCTION_ACCOUNT {
                RuntimeValue::Pubkey(account.key.to_bytes())
            } else {
                RuntimeValue::U64(
                    u64::from(account.is_signer()) | u64::from(account.is_writable()) << 1,
                )
            }
        }
        OP_INSTRUCTION_DATA_LEN => {
            RuntimeValue::U64(introspected.get_instruction_data().len() as u64)
        }
        OP_READ_INSTRUCTION_DATA => {
            // The verifier admits only read opcodes, which all fit a byte.
            let selector = instruction.immediate() as u8;
            let width = read_width(selector);
            if width == 0 {
                return Err(BallistaError::InvalidTemplateProgram.into());
            }
            let offset = position(registers, instruction.c)?;
            let data = introspected.get_instruction_data();
            // Checked here so a short read is out of range, not `read_value`'s account error.
            byte_range(data, offset, width as u64)?;
            read_value(selector, data, offset)?
        }
        OP_READ_INSTRUCTION_BYTES => {
            let data =
                reborrow(sysvar, introspected.get_instruction_data()).ok_or_else(out_of_range)?;
            RuntimeValue::Bytes(byte_range(
                data,
                position(registers, instruction.c)?,
                instruction.immediate(),
            )?)
        }
        _ => return Err(BallistaError::InvalidTemplateProgram.into()),
    })
}

/// The Instructions sysvar's data, borrowed for the rest of the run.
fn sysvar_data(account: &AccountView) -> RunResult<&[u8]> {
    // The verifier pinned this account to the sysvar, and the run checked the pin before the first
    // instruction. Checked again because the borrow below is sound for this one account only.
    if account.address() != &INSTRUCTIONS_ID {
        return Err(BallistaError::InvalidRuntimeAccount.into());
    }
    // SAFETY: the runtime builds this account's data from the transaction before any program runs,
    // and never resizes it. A sysvar is read-only in every transaction, so no program can write it
    // or borrow it mutably. The runtime itself rewrites only the trailing current-instruction
    // index, and while this instruction runs, inside any CPI it makes too, the index it writes is
    // this instruction's own: the bytes never change. `borrow_unchecked` leaves the borrow flag
    // alone, so holding the slice across a CPI cannot make that CPI's borrow check fail.
    Ok(unsafe { account.borrow_unchecked() })
}

/// The data of an account this instruction cannot write, borrowed for the rest of the run.
fn read_only_data(account: &AccountView) -> RunResult<&[u8]> {
    if account.is_writable() {
        return Err(BallistaError::WritableAccountBytesRead.into());
    }
    // SAFETY: the account is read-only in this instruction, which is what `is_writable` reports.
    // When another program called Ballista through a CPI, the transaction may still be able to
    // write it, but nothing writes it or changes its length while this instruction runs:
    // - Ballista cannot, and a program Ballista calls cannot gain a privilege Ballista lacks, so
    //   the account is read-only in every CPI made from here.
    // - The program that called Ballista is paused until Ballista returns, and cannot be
    //   re-entered from any CPI Ballista makes.
    // - After a CPI, the runtime copies data back only into accounts that were writable in it.
    // So the bytes stay as they are for the rest of the run. `borrow_unchecked` leaves the borrow
    // flag alone, so a later CPI that passes this account still passes its borrow check.
    Ok(unsafe { account.borrow_unchecked() })
}

/// A `u64` register used as an instruction index, account position or byte offset.
fn position(registers: &[RuntimeValue<'_>], register: u8) -> RunResult<usize> {
    match get(registers, register)? {
        RuntimeValue::U64(value) => usize::try_from(value).map_err(|_| out_of_range()),
        _ => Err(BallistaError::TypeMismatch.into()),
    }
}

/// Exactly `len` bytes of `data` from `offset`, or `InstructionOutOfRange` when they are not all
/// there.
fn byte_range(data: &[u8], offset: usize, len: u64) -> RunResult<&[u8]> {
    let len = usize::try_from(len).map_err(|_| BallistaError::InvalidTemplateProgram)?;
    offset
        .checked_add(len)
        .and_then(|end| data.get(offset..end))
        .ok_or_else(out_of_range)
}

/// `part`, a slice inside `whole`, as a borrow of `whole`. pinocchio ties every slice it returns
/// to a borrow of its parser, which ends with the opcode; the bytes live as long as `whole`.
/// Bounds-checked, so a `part` from anywhere else gives `None` instead of an alias.
fn reborrow<'data>(whole: &'data [u8], part: &[u8]) -> Option<&'data [u8]> {
    let start = (part.as_ptr() as usize).checked_sub(whole.as_ptr() as usize)?;
    whole.get(start..start.checked_add(part.len())?)
}

fn out_of_range() -> RunError {
    BallistaError::InstructionOutOfRange.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio::account::{RuntimeAccount, NOT_BORROWED};
    use pinocchio::Address;
    use RuntimeValue::{Bytes, Pubkey, Unset, U64};

    /// One instruction as the sysvar lists it: program, `(key, signer, writable)` accounts, data.
    type Listed<'a> = ([u8; 32], &'a [([u8; 32], bool, bool)], &'a [u8]);

    /// The Instructions sysvar's data for `instructions`, laid out as `construct_instructions_data`
    /// writes it, with `current` as the running instruction's index.
    fn sysvar(instructions: &[Listed<'_>], current: u16) -> Vec<u8> {
        let mut data = (instructions.len() as u16).to_le_bytes().to_vec();
        data.resize(2 + 2 * instructions.len(), 0);
        for (index, (program, accounts, bytes)) in instructions.iter().enumerate() {
            let start = data.len() as u16;
            data[2 + 2 * index..4 + 2 * index].copy_from_slice(&start.to_le_bytes());
            data.extend_from_slice(&(accounts.len() as u16).to_le_bytes());
            for (key, signer, writable) in *accounts {
                data.push(u8::from(*signer) | u8::from(*writable) << 1);
                data.extend_from_slice(key);
            }
            data.extend_from_slice(program);
            data.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            data.extend_from_slice(bytes);
        }
        data.extend_from_slice(&current.to_le_bytes());
        data
    }

    fn err(kind: BallistaError) -> RunError {
        RunError::Vm(kind)
    }

    /// A memo signed by `[1; 32]`, this template's run with a writable `[2; 32]`, and a bare memo.
    fn three_instructions() -> Vec<u8> {
        sysvar(
            &[
                ([5; 32], &[([1; 32], true, false)], b"before"),
                ([9; 32], &[([2; 32], false, true), ([1; 32], true, false)], &[0x2a]),
                ([5; 32], &[], b"after"),
            ],
            1,
        )
    }

    /// Runs `opcode` with the instruction index in r0 and the position or offset in r1.
    fn run<'a>(
        sysvar: &'a [u8],
        registers: &[RuntimeValue<'a>],
        opcode: u8,
        immediate: u64,
    ) -> RunResult<RuntimeValue<'a>> {
        // SAFETY: every caller passes `three_instructions()`, which `sysvar` lays out as the
        // runtime does.
        unsafe { introspect(sysvar, registers, &record(opcode, 3, 0, 0, 1, 0, immediate)) }
    }

    #[test]
    fn counts_and_indexes_come_from_the_sysvar() {
        let data = three_instructions();
        assert_eq!(run(&data, &[U64(0), U64(0)], OP_INSTRUCTION_COUNT, 0), Ok(U64(3)));
        assert_eq!(run(&data, &[U64(0), U64(0)], OP_INSTRUCTION_INDEX, 0), Ok(U64(1)));
    }

    #[test]
    fn instruction_fields_read_the_indexed_instruction() {
        let data = three_instructions();
        // r0 is the instruction index, r1 the account position or data offset.
        assert_eq!(run(&data, &[U64(0), U64(0)], OP_INSTRUCTION_PROGRAM, 0), Ok(Pubkey([5; 32])));
        assert_eq!(run(&data, &[U64(1), U64(0)], OP_INSTRUCTION_PROGRAM, 0), Ok(Pubkey([9; 32])));
        assert_eq!(run(&data, &[U64(1), U64(0)], OP_INSTRUCTION_ACCOUNT_COUNT, 0), Ok(U64(2)));
        assert_eq!(run(&data, &[U64(2), U64(0)], OP_INSTRUCTION_ACCOUNT_COUNT, 0), Ok(U64(0)));
        assert_eq!(run(&data, &[U64(1), U64(1)], OP_INSTRUCTION_ACCOUNT, 0), Ok(Pubkey([1; 32])));
        assert_eq!(run(&data, &[U64(1), U64(0)], OP_INSTRUCTION_ACCOUNT_FLAGS, 0), Ok(U64(0b10)));
        assert_eq!(run(&data, &[U64(1), U64(1)], OP_INSTRUCTION_ACCOUNT_FLAGS, 0), Ok(U64(0b01)));
        assert_eq!(run(&data, &[U64(0), U64(0)], OP_INSTRUCTION_DATA_LEN, 0), Ok(U64(6)));
        assert_eq!(run(&data, &[U64(2), U64(0)], OP_INSTRUCTION_DATA_LEN, 0), Ok(U64(5)));
    }

    #[test]
    fn instruction_data_reads_take_the_selected_width() {
        let data = three_instructions();
        // "before" is 62 65 66 6f 72 65.
        assert_eq!(
            run(&data, &[U64(0), U64(1)], OP_READ_INSTRUCTION_DATA, OP_READ_U8 as u64),
            Ok(U64(0x65))
        );
        assert_eq!(
            run(&data, &[U64(0), U64(0)], OP_READ_INSTRUCTION_DATA, OP_READ_U32 as u64),
            Ok(U64(0x6f66_6562))
        );
        // The last two bytes fit; one more does not.
        assert_eq!(
            run(&data, &[U64(0), U64(4)], OP_READ_INSTRUCTION_DATA, OP_READ_U16 as u64),
            Ok(U64(0x6572))
        );
        assert_eq!(
            run(&data, &[U64(0), U64(5)], OP_READ_INSTRUCTION_DATA, OP_READ_U16 as u64),
            Err(err(BallistaError::InstructionOutOfRange))
        );
        assert_eq!(
            run(&data, &[U64(0), U64(0)], OP_READ_INSTRUCTION_DATA, OP_ADD as u64),
            Err(err(BallistaError::InvalidTemplateProgram))
        );
    }

    #[test]
    fn instruction_byte_reads_borrow_the_sysvar() {
        let data = three_instructions();
        let value = run(&data, &[U64(2), U64(0)], OP_READ_INSTRUCTION_BYTES, 5).unwrap();
        let Bytes(bytes) = value else { panic!("{value:?}") };
        assert_eq!(bytes, b"after");
        // A slice of the sysvar itself, not a copy: its last byte sits just before the index.
        assert_eq!(bytes.as_ptr_range().end, data[data.len() - 2..].as_ptr());
        assert_eq!(
            run(&data, &[U64(2), U64(1)], OP_READ_INSTRUCTION_BYTES, 5),
            Err(err(BallistaError::InstructionOutOfRange))
        );
        assert_eq!(
            run(&data, &[U64(0), U64(u64::MAX)], OP_READ_INSTRUCTION_BYTES, 1),
            Err(err(BallistaError::InstructionOutOfRange))
        );
    }

    #[test]
    fn indexes_and_positions_outside_the_transaction_fail() {
        let data = three_instructions();
        let out_of_range = Err(err(BallistaError::InstructionOutOfRange));
        for opcode in [OP_INSTRUCTION_PROGRAM, OP_INSTRUCTION_ACCOUNT_COUNT, OP_INSTRUCTION_DATA_LEN] {
            assert_eq!(run(&data, &[U64(3), U64(0)], opcode, 0), out_of_range, "{opcode}");
            assert_eq!(run(&data, &[U64(u64::MAX), U64(0)], opcode, 0), out_of_range, "{opcode}");
        }
        assert_eq!(run(&data, &[U64(1), U64(2)], OP_INSTRUCTION_ACCOUNT, 0), out_of_range);
        assert_eq!(run(&data, &[U64(2), U64(0)], OP_INSTRUCTION_ACCOUNT_FLAGS, 0), out_of_range);
        // An operand that is not a set u64 is the template's fault, not the transaction's.
        assert_eq!(
            run(&data, &[Unset, U64(0)], OP_INSTRUCTION_PROGRAM, 0),
            Err(err(BallistaError::InvalidRegister))
        );
        assert_eq!(
            run(&data, &[Pubkey([0; 32]), U64(0)], OP_INSTRUCTION_PROGRAM, 0),
            Err(err(BallistaError::TypeMismatch))
        );
    }

    #[test]
    fn bytes_len_measures_a_bytes_register() {
        assert_eq!(bytes_len(&Bytes(&[1, 2, 3])), Ok(3));
        assert_eq!(bytes_len(&Bytes(&[])), Ok(0));
        assert_eq!(bytes_len(&U64(3)), Err(err(BallistaError::TypeMismatch)));
        assert_eq!(bytes_len(&Unset), Err(err(BallistaError::InvalidRegister)));
    }

    #[test]
    fn a_slice_is_reborrowed_only_from_inside_its_parent() {
        let whole = [1u8, 2, 3, 4];
        assert_eq!(reborrow(&whole, &whole[1..3]), Some(&whole[1..3]));
        let elsewhere = [1u8, 2];
        assert_eq!(reborrow(&whole[1..], &elsewhere), None);
    }

    /// An account laid out as the entrypoint hands it over: the runtime header, then its data.
    struct TestAccount {
        buffer: Vec<u64>,
    }

    impl TestAccount {
        fn new(address: [u8; 32], writable: bool, data: &[u8]) -> Self {
            let header = core::mem::size_of::<RuntimeAccount>();
            let mut buffer = vec![0u64; (header + data.len()).div_ceil(8)];
            let account = RuntimeAccount {
                borrow_state: NOT_BORROWED,
                is_signer: 0,
                is_writable: u8::from(writable),
                executable: 0,
                padding: [0; 4],
                address: Address::new_from_array(address),
                owner: Address::new_from_array([0; 32]),
                lamports: 0,
                data_len: data.len() as u64,
            };
            // SAFETY: the buffer is eight-aligned and large enough for the header and the data.
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

    #[test]
    fn account_bytes_come_only_from_read_only_accounts_and_hold_no_borrow() {
        // Read three bytes from the offset in r0 into r1.
        let read = record(OP_READ_ACCOUNT_BYTES, 1, 0, 0, NO_INDEX, 0, 3);

        let mut read_only = TestAccount::new([4; 32], false, &[10, 11, 12, 13, 14]);
        let accounts = [read_only.view()];
        let mut registers = vec![U64(1), Unset];
        assert_eq!(read_account_bytes(&accounts[0], &mut registers, &read), Ok(()));
        assert_eq!(registers[1], Bytes(&[11, 12, 13]));
        assert!(!accounts[0].is_borrowed(), "the borrow flag is untouched");

        // A range past the end.
        let mut registers = vec![U64(3), Unset];
        assert_eq!(
            read_account_bytes(&accounts[0], &mut registers, &read),
            Err(err(BallistaError::InstructionOutOfRange))
        );

        let mut writable = TestAccount::new([4; 32], true, &[10, 11, 12, 13, 14]);
        let accounts = [writable.view()];
        let mut registers = vec![U64(1), Unset];
        assert_eq!(
            read_account_bytes(&accounts[0], &mut registers, &read),
            Err(err(BallistaError::WritableAccountBytesRead))
        );
        assert_eq!(registers[1], Unset);
    }

    #[test]
    fn introspection_refuses_any_account_but_the_sysvar() {
        // Read the program of the instruction in r0 into r1.
        let program = record(OP_INSTRUCTION_PROGRAM, 1, 0, 0, NO_INDEX, 0, 0);
        let data = three_instructions();

        let mut real = TestAccount::new(INSTRUCTIONS_SYSVAR_ID, false, &data);
        let sysvar = real.view();
        assert_eq!(count_or_index(OP_INSTRUCTION_COUNT, &sysvar), Ok(3));
        assert_eq!(count_or_index(OP_INSTRUCTION_INDEX, &sysvar), Ok(1));
        let mut registers = vec![U64(2), Unset];
        assert_eq!(read_instruction(&sysvar, &mut registers, &program), Ok(()));
        assert_eq!(registers[1], Pubkey([5; 32]));
        assert!(!sysvar.is_borrowed(), "the borrow flag is untouched");

        let mut impostor = TestAccount::new([6; 32], false, &data);
        let impostor = impostor.view();
        assert_eq!(
            count_or_index(OP_INSTRUCTION_COUNT, &impostor),
            Err(err(BallistaError::InvalidRuntimeAccount))
        );
        assert_eq!(
            read_instruction(&impostor, &mut registers, &program),
            Err(err(BallistaError::InvalidRuntimeAccount))
        );
    }

    #[test]
    fn the_sysvar_address_matches_pinocchio() {
        assert_eq!(INSTRUCTIONS_ID.to_bytes(), INSTRUCTIONS_SYSVAR_ID);
    }
}
