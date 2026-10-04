//! Coverage-guided native fuzzing of the bytecode executor.
//!
//! libFuzzer's mutations drive [`ballista_fuzz_gen`]'s generators to build a template that verifies
//! and a run for it, then the executor runs on the host through the program's `spec-api` feature.
//! Off-chain, pinocchio stubs every CPI, the return-data calls and the sysvars, so this exercises
//! the interpreter's own branches — `dispatch`, `invoke_cpi`, `derive_pda`, the math and the
//! registry code — with CPIs stubbed. The property is that the executor never panics, aborts, or
//! reads out of bounds: any `Result` it returns is fine; a crash is a finding libFuzzer records.
#![no_main]

use ballista::processor::execute;
use ballista_common::template::ProgramView;
use ballista_fuzz_gen::scenario::{generate_scenario, Kind};
use ballista_fuzz_gen::source::{ByteSource, Gen};
use ballista_fuzz_gen::template::{generate_template, World};
use libfuzzer_sys::fuzz_target;
use pinocchio::account::{AccountView, RuntimeAccount, NOT_BORROWED};
use solana_address::Address;

const BALLISTA: [u8; 32] = [11; 32];
const PROBE: [u8; 32] = [12; 32];
const TOKEN: [u8; 32] = [13; 32];

/// One account laid out as the runtime hands it over: the header, then its data, eight-aligned.
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

/// A well-formed Instructions sysvar holding one instruction, so introspection reads are defined.
/// Off-chain pinocchio parses this through raw pointers, so it must be valid and 2-aligned (the
/// `AccountBuf` words are eight-aligned).
fn valid_sysvar_data() -> Vec<u8> {
    let mut data = 1u16.to_le_bytes().to_vec(); // one instruction
    data.extend_from_slice(&0u16.to_le_bytes()); // placeholder for the offset table entry
    let start = data.len() as u16;
    data[2..4].copy_from_slice(&start.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes()); // zero accounts
    data.extend_from_slice(&BALLISTA); // program id
    data.extend_from_slice(&1u16.to_le_bytes()); // data length 1
    data.push(5); // IX_RUN
    data.extend_from_slice(&0u16.to_le_bytes()); // current instruction index
    data
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 8 {
        return;
    }
    let world = World::new(BALLISTA, PROBE, TOKEN);
    let mut source = ByteSource::new(data);
    let mut gen = Gen::new(&mut source);
    let plan = generate_template(&mut gen, &world);

    // The template must verify (the generator's contract); skip the rare fallback or a malformed
    // one rather than feed the executor something the program would reject at upload.
    let program = match ProgramView::parse(&plan.bytes) {
        Ok(program) if program.verify().is_ok() => program,
        _ => return,
    };

    // A deterministic template address, and the scenario's accounts and run data.
    let template_address = {
        let (address, _) = Address::find_program_address(&[b"template", &[0x11; 32], &1u16.to_le_bytes()], &Address::new_from_array(BALLISTA));
        address.to_bytes()
    };
    let pda = |seeds: &[&[u8]], program: &[u8; 32]| {
        let (address, bump) = Address::find_program_address(seeds, &Address::new_from_array(*program));
        (address.to_bytes(), bump)
    };
    let rent = |len: usize| ((128 + len) as u64).saturating_mul(6_960);
    let scenario = generate_scenario(&mut gen, &world, &plan, template_address, &pda, &rent);

    // Build the account buffers: the template first, then the runtime accounts in slot order. The
    // Instructions sysvar gets valid data so off-chain introspection is defined.
    let mut bufs: Vec<AccountBuf> = Vec::with_capacity(scenario.slots.len() + 1);
    let spec_buf = |spec: &ballista_fuzz_gen::scenario::AccountSpec| -> AccountBuf {
        let sysvar = matches!(spec.kind, Kind::Sysvar);
        let data = if sysvar { valid_sysvar_data() } else { spec.data.clone() };
        AccountBuf::new(spec.address, spec.owner, spec.lamports, spec.signer, spec.writable, spec.executable, &data)
    };
    bufs.push(spec_buf(&scenario.pool[scenario.template]));
    for &index in &scenario.slots {
        bufs.push(spec_buf(&scenario.pool[index]));
    }
    let accounts: Vec<AccountView> = bufs.iter_mut().map(|buf| buf.view()).collect();

    // Input bytes are the run data without the `IX_RUN` discriminator.
    let input = scenario.run_data.get(1..).unwrap_or(&[]);

    // CPIs are no-ops off-chain, so this drives the interpreter end to end. Any Result is fine; a
    // panic or out-of-bounds access aborts and libFuzzer records the crash.
    let _ = execute::run(&program, input, &accounts);
});
