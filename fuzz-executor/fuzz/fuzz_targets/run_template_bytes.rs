//! Coverage-guided native fuzzing of the executor on raw template bytes, seeded from the real
//! templates in `fixtures/` (the compiler's fixtures, the protocol examples and the benchmarks).
//!
//! The input is a template payload. One that parses and verifies runs on the host through
//! `spec-api`, against accounts synthesized from its own declarations and run data drawn from a
//! generator seeded by the payload's hash, so a mutation of the template changes everything
//! deterministically. CPIs are stubbed off-chain. The property is no panic and no out-of-bounds
//! access; any `Result` is fine.
#![no_main]

use ballista::processor::execute;
use ballista_common::template::*;
use ballista_fuzz_gen::source::{Gen, SplitMix64};
use libfuzzer_sys::fuzz_target;
use pinocchio::account::{AccountView, RuntimeAccount, NOT_BORROWED};
use solana_address::Address;

struct AccountBuf {
    words: Vec<u64>,
}

impl AccountBuf {
    fn new(address: [u8; 32], owner: [u8; 32], lamports: u64, signer: bool, writable: bool, executable: bool, data: &[u8]) -> Self {
        let header = core::mem::size_of::<RuntimeAccount>();
        let mut words = vec![0u64; (header + data.len()).div_ceil(8)];
        let account = RuntimeAccount {
            borrow_state: NOT_BORROWED,
            is_signer: u8::from(signer),
            is_writable: u8::from(writable),
            executable: u8::from(executable),
            padding: [0; 4],
            address: Address::new_from_array(address),
            owner: Address::new_from_array(owner),
            lamports,
            data_len: data.len() as u64,
        };
        // SAFETY: `words` is eight-aligned and holds the header followed by the data.
        unsafe {
            core::ptr::write(words.as_mut_ptr().cast::<RuntimeAccount>(), account);
            core::ptr::copy_nonoverlapping(data.as_ptr(), words.as_mut_ptr().cast::<u8>().add(header), data.len());
        }
        Self { words }
    }

    fn view(&mut self) -> AccountView {
        // SAFETY: `new` wrote a runtime account header followed by its data.
        unsafe { AccountView::new_unchecked(self.words.as_mut_ptr().cast()) }
    }
}

/// A well-formed Instructions sysvar listing one instruction, so off-chain introspection (which
/// pinocchio parses through raw pointers) only ever sees valid layout.
fn valid_sysvar_data() -> Vec<u8> {
    let mut data = 1u16.to_le_bytes().to_vec();
    data.extend_from_slice(&4u16.to_le_bytes()); // the one instruction starts at offset 4
    data.extend_from_slice(&0u16.to_le_bytes()); // no accounts
    data.extend_from_slice(&[11; 32]); // program id
    data.extend_from_slice(&1u16.to_le_bytes());
    data.push(5);
    data.extend_from_slice(&0u16.to_le_bytes()); // current index
    data
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ *byte as u64).wrapping_mul(0x100_0000_01b3))
}

/// An account satisfying `constraint`, mostly; `index` makes its address unique.
fn synthesize(program: &ProgramView, constraint: &AccountConstraint, index: usize, g: &mut Gen) -> AccountBuf {
    let pubkey = |i: u8| program.pubkeys.get(i as usize).map(|record| record.bytes);
    let mut address = [0u8; 32];
    address[0] = 0xf0;
    address[1..9].copy_from_slice(&(index as u64).to_le_bytes());
    let address = if constraint.address_index != NO_INDEX { pubkey(constraint.address_index).unwrap_or(address) } else { address };
    let owner = if constraint.owner_index != NO_INDEX { pubkey(constraint.owner_index).unwrap_or([0; 32]) } else { [0; 32] };
    let data = if address == INSTRUCTIONS_SYSVAR_ID {
        valid_sysvar_data()
    } else {
        // `min_data_len` is a u32 the verifier does not bound, and a mutated template can declare
        // gigabytes. No real account holds that much, so cap the synthesized data: a larger
        // declaration then meets a short account and fails validation, as it would on chain.
        const MAX_SYNTHESIZED: usize = 16 * 1024;
        let len = constraint.min_data_len().min(MAX_SYNTHESIZED) + g.below(24);
        g.bytes(len)
    };
    // Mostly grant what the declaration requires; sometimes withhold it to exercise rejection.
    let honest = !g.chance(1, 10);
    AccountBuf::new(
        address,
        owner,
        1_000_000_000,
        honest && constraint.flags & ACCOUNT_SIGNER != 0,
        honest && constraint.flags & ACCOUNT_WRITABLE != 0,
        constraint.flags & ACCOUNT_EXECUTABLE != 0,
        &data,
    )
}

fn encode_input(descriptor: &InputDescriptor, g: &mut Gen, out: &mut Vec<u8>) {
    match descriptor.value_type {
        VALUE_BOOL => out.push(g.below(2) as u8),
        VALUE_U64 => out.extend_from_slice(&g.interesting_u64().to_le_bytes()),
        VALUE_I64 => out.extend_from_slice(&g.interesting_i64().to_le_bytes()),
        VALUE_U128 => out.extend_from_slice(&g.interesting_u128().to_le_bytes()),
        VALUE_PUBKEY => out.extend_from_slice(&g.bytes(32)),
        _ => {
            let len = g.below(descriptor.max_len() + 1);
            out.extend_from_slice(&(len as u16).to_le_bytes());
            out.extend_from_slice(&g.bytes(len));
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(program) = ProgramView::parse(data) else { return };
    if program.verify().is_err() {
        return;
    }
    let mut source = SplitMix64::new(fnv(data));
    let mut g = Gen::new(&mut source);
    let header = program.header;
    let fixed = header.fixed_account_count();
    let stride = header.batch_stride();

    let mut bufs = vec![AccountBuf::new([0x7e; 32], [11; 32], 1, false, false, false, &[])];
    for index in 0..fixed {
        bufs.push(synthesize(&program, &program.accounts[index], index, &mut g));
    }
    let iterations = if stride == 0 {
        0
    } else {
        g.range(header.batch_min_iterations(), header.batch_max_iterations())
    };
    for row in 0..iterations {
        for offset in 0..stride {
            let index = fixed + row * stride + offset;
            bufs.push(synthesize(&program, &program.accounts[fixed + offset], index, &mut g));
        }
    }
    let mut run_data = Vec::new();
    for _ in 0..header.account_group_count() {
        let len = g.below(3);
        run_data.push(len as u8);
        for member in 0..len {
            let index = 0x1000 + bufs.len() + member;
            bufs.push(AccountBuf::new([index as u8; 32], [0; 32], 1, false, g.chance(1, 2), false, &[]));
        }
    }
    for descriptor in &program.inputs[..header.input_count().min(program.inputs.len())] {
        encode_input(descriptor, &mut g, &mut run_data);
    }
    let row_inputs = program.inputs.get(header.input_count()..).unwrap_or(&[]);
    for _ in 0..iterations {
        for descriptor in row_inputs {
            encode_input(descriptor, &mut g, &mut run_data);
        }
    }
    let accounts: Vec<AccountView> = bufs.iter_mut().map(|buf| buf.view()).collect();
    let _ = execute::run(&program, &run_data, &accounts);
});
