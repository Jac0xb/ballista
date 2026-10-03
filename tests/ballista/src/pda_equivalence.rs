//! `DERIVE_PDA` and `CREATE_PDA` on chain, against the address crate on the host.
//!
//! The executor derives addresses from `sol_sha256` and `sol_curve_validate_point` rather than the
//! runtime's PDA syscalls. These tests run both opcodes through real templates under Mollusk and
//! require every result to equal `Pubkey::find_program_address` or `create_program_address`
//! computed on the host: one to fifteen seeds of 0 to 32 bytes, random program ids, canonical
//! bumps many attempts deep, supplied bumps that land on the curve, bump zero, and bumps that do
//! not fit in a byte. A template compares its result with an expected-address input and fails the
//! run if they differ, so each case checks the on-chain address itself, not just success.
#![cfg(test)]

use ballista_common::instruction::IX_RUN;
use ballista_common::template::{
    ProgramBuilder, Segment, TemplateAccountHeader, ACCOUNT_EXECUTABLE, DATA_REG_BYTES,
    DATA_REG_PUBKEY, DATA_REG_U16, DATA_REG_U64, DATA_REG_U8, MAX_PDA_SEEDS, MAX_PDA_SEED_LEN,
    OP_EQ, VALUE_BYTES, VALUE_PUBKEY, VALUE_U64,
};
use mollusk_svm::{
    program::loader_keys::LOADER_V3,
    result::{InstructionResult, ProgramResult},
    Mollusk,
};
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_program_error::ProgramError;
use solana_pubkey::Pubkey;

use crate::cases::{mollusk, ID};

const TEMPLATE_SEED: &[u8] = b"template";
const REQUIREMENT_FAILED: u32 = 6015;
const INVALID_PDA_DERIVATION: u32 = 6017;

/// A small deterministic generator, so a failure names a reproducible case.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }

    fn pubkey(&mut self) -> Pubkey {
        Pubkey::new_from_array(self.bytes(32).try_into().unwrap())
    }

    /// `count` seeds, each 0 to 32 bytes long.
    fn seeds(&mut self, count: usize) -> Vec<Vec<u8>> {
        (0..count)
            .map(|_| {
                let len = self.below(MAX_PDA_SEED_LEN as u64 + 1) as usize;
                self.bytes(len)
            })
            .collect()
    }
}

/// Canonical search depth: 1 when bump 255 is off the curve, 2 for 254, and so on.
fn depth(bump: u8) -> u32 {
    256 - bump as u32
}

fn slices(seeds: &[Vec<u8>]) -> Vec<&[u8]> {
    seeds.iter().map(Vec::as_slice).collect()
}

/// A template deriving from `count` bytes inputs that requires the result to equal the next input.
/// Inputs: the seeds, the expected address, then (for `CREATE_PDA`) the bump.
fn bytes_seed_template(count: usize, supplied_bump: bool) -> Vec<u8> {
    let mut builder = ProgramBuilder::new();
    let program = builder.account(ACCOUNT_EXECUTABLE, None, None, 0);
    let seed_inputs: Vec<u8> = (0..count)
        .map(|_| builder.input(VALUE_BYTES, MAX_PDA_SEED_LEN as u16))
        .collect();
    let expected_input = builder.input(VALUE_PUBKEY, 0);
    let bump_input = supplied_bump.then(|| builder.input(VALUE_U64, 0));
    let seeds: Vec<Segment> = seed_inputs
        .iter()
        .map(|input| Segment::Register(DATA_REG_BYTES, builder.load_input(*input)))
        .collect();
    let expected = builder.load_input(expected_input);
    let derived = match bump_input {
        Some(input) => {
            let bump = builder.load_input(input);
            builder.create_pda(program, bump, &seeds)
        }
        None => builder.derive_pda(program, &seeds),
    };
    let same = builder.binary(OP_EQ, derived, expected);
    builder.require(same);
    builder.build().expect("builds")
}

fn bytes_seed_inputs(seeds: &[Vec<u8>], expected: &Pubkey, bump: Option<u64>) -> Vec<u8> {
    let mut inputs = Vec::new();
    for seed in seeds {
        inputs.extend_from_slice(&(seed.len() as u16).to_le_bytes());
        inputs.extend_from_slice(seed);
    }
    inputs.extend_from_slice(expected.as_ref());
    if let Some(bump) = bump {
        inputs.extend_from_slice(&bump.to_le_bytes());
    }
    inputs
}

/// Runs templates against one Mollusk instance, each stored as a finalized template account.
struct Runner {
    mollusk: Mollusk,
    creator: Pubkey,
    templates: Vec<(Pubkey, Account)>,
}

impl Runner {
    fn new() -> Self {
        Self {
            mollusk: mollusk(),
            creator: Pubkey::new_from_array([5; 32]),
            templates: Vec::new(),
        }
    }

    /// Stores a finalized template and returns its index.
    fn add(&mut self, payload: &[u8]) -> usize {
        let id = self.templates.len() as u16;
        let (address, bump) = Pubkey::find_program_address(
            &[TEMPLATE_SEED, self.creator.as_ref(), &id.to_le_bytes()],
            &ID,
        );
        let hash = solana_sha256_hasher::hash(payload).to_bytes();
        let mut header = TemplateAccountHeader::new_uploading(
            self.creator.to_bytes(),
            id,
            bump,
            payload.len(),
            hash,
        )
        .expect("header");
        header.set_written_len(payload.len()).expect("written len");
        header.finalize().expect("finalize");
        let mut data = header.as_bytes().to_vec();
        data.extend_from_slice(payload);
        let mut account = Account::new(10_000_000, data.len(), &ID);
        account.data = data;
        self.templates.push((address, account));
        self.templates.len() - 1
    }

    /// Runs template `index` with `program` as its PDA program account.
    fn run(&self, index: usize, program: &(Pubkey, Account), inputs: &[u8]) -> InstructionResult {
        let (template, template_account) = &self.templates[index];
        let mut data = vec![IX_RUN];
        data.extend_from_slice(inputs);
        let instruction = Instruction {
            program_id: ID,
            accounts: vec![
                AccountMeta::new_readonly(*template, false),
                AccountMeta::new_readonly(program.0, false),
            ],
            data,
        };
        self.mollusk.process_instruction(
            &instruction,
            &[(*template, template_account.clone()), program.clone()],
        )
    }
}

/// The error kind a run failed with, without its location.
fn error_kind(result: &InstructionResult) -> Option<u32> {
    match &result.program_result {
        ProgramResult::Failure(ProgramError::Custom(code)) => Some(code & 0xffff),
        _ => None,
    }
}

/// Executable accounts to derive against: the System Program, whose all-zero id is a real edge,
/// and programs at random addresses.
fn programs(rng: &mut SplitMix, count: usize) -> Vec<(Pubkey, Account)> {
    let mut programs = vec![mollusk_svm::program::keyed_account_for_system_program()];
    programs.extend((1..count).map(|_| {
        let account = Account {
            lamports: 1_000_000,
            data: Vec::new(),
            owner: LOADER_V3,
            executable: true,
            rent_epoch: 0,
        };
        (rng.pubkey(), account)
    }));
    programs
}

#[test]
fn derive_pda_matches_find_program_address() {
    const CASES_PER_COUNT: usize = 40;
    let mut rng = SplitMix(0x0bad_5eed);
    let programs = programs(&mut rng, 8);
    let mut runner = Runner::new();
    let templates: Vec<usize> = (1..=MAX_PDA_SEEDS)
        .map(|count| runner.add(&bytes_seed_template(count, false)))
        .collect();

    let mut runs = 0;
    let mut depths = [0usize; 17];
    for (index, count) in (1..=MAX_PDA_SEEDS).enumerate() {
        for case in 0..CASES_PER_COUNT {
            let seeds = rng.seeds(count);
            let program = &programs[rng.below(programs.len() as u64) as usize];
            let (expected, bump) = Pubkey::find_program_address(&slices(&seeds), &program.0);
            depths[(depth(bump) as usize).min(16)] += 1;

            let result = runner.run(
                templates[index],
                program,
                &bytes_seed_inputs(&seeds, &expected, None),
            );
            assert!(
                result.program_result.is_ok(),
                "{count} seeds, case {case}, bump {bump}: {:?}",
                result.program_result
            );
            runs += 1;

            // Every eighth case also runs with one bit of the expected address flipped, which the
            // template must reject: the comparison is live, so a wrong derivation would fail.
            if case % 8 == 0 {
                let mut wrong = expected.to_bytes();
                wrong[(case / 8) % 32] ^= 1;
                let result = runner.run(
                    templates[index],
                    program,
                    &bytes_seed_inputs(&seeds, &Pubkey::new_from_array(wrong), None),
                );
                assert_eq!(error_kind(&result), Some(REQUIREMENT_FAILED), "{result:?}");
                runs += 1;
            }
        }
    }
    let deepest = depths.iter().rposition(|n| *n > 0).unwrap();
    eprintln!(
        "DERIVE_PDA: {runs} runs over 1-15 seeds; canonical depth histogram (attempts: cases) {:?}",
        depths
            .iter()
            .enumerate()
            .filter(|(_, n)| **n > 0)
            .collect::<Vec<_>>()
    );
    assert!(deepest >= 6, "some searches go six attempts deep");
}

/// Seeds found on the host whose canonical bump is at least ten attempts deep, run on chain.
#[test]
fn derive_pda_matches_deep_searches() {
    let mut rng = SplitMix(0xdee9);
    let programs = programs(&mut rng, 4);
    let mut runner = Runner::new();
    let mut deepest = 0;
    let mut runs = 0;
    for count in [1, 2, 3, 8, 15] {
        let template = runner.add(&bytes_seed_template(count, false));
        let mut found = 0;
        while found < 2 {
            let seeds = rng.seeds(count);
            let program = &programs[found % programs.len()];
            let (expected, bump) = Pubkey::find_program_address(&slices(&seeds), &program.0);
            if depth(bump) < 10 {
                continue;
            }
            let result = runner.run(
                template,
                program,
                &bytes_seed_inputs(&seeds, &expected, None),
            );
            assert!(
                result.program_result.is_ok(),
                "{count} seeds, bump {bump}: {:?}",
                result.program_result
            );
            deepest = deepest.max(depth(bump));
            found += 1;
            runs += 1;
        }
    }
    eprintln!("DERIVE_PDA deep searches: {runs} runs, up to {deepest} attempts");
}

#[test]
fn create_pda_matches_create_program_address() {
    const CASES_PER_COUNT: usize = 24;
    let mut rng = SplitMix(0xc4ea_7e);
    let programs = programs(&mut rng, 8);
    let mut runner = Runner::new();
    let templates: Vec<usize> = (1..=MAX_PDA_SEEDS)
        .map(|count| runner.add(&bytes_seed_template(count, true)))
        .collect();

    let (mut accepted, mut on_curve, mut oversized) = (0, 0, 0);
    for (index, count) in (1..=MAX_PDA_SEEDS).enumerate() {
        for case in 0..CASES_PER_COUNT {
            let seeds = rng.seeds(count);
            let program = &programs[rng.below(programs.len() as u64) as usize];
            let (_, canonical) = Pubkey::find_program_address(&slices(&seeds), &program.0);
            // The canonical bump, every bump above it up to four (all on the curve), a random
            // bump that may land either side, and zero, which a search never tries.
            let above = (canonical as u16 + 1..=255).take(4).map(|bump| bump as u8);
            let bumps: Vec<u8> = [canonical, rng.next() as u8, 0]
                .into_iter()
                .chain(above)
                .collect();
            for bump in bumps {
                let mut with_bump = slices(&seeds);
                let bump_seed = [bump];
                with_bump.push(&bump_seed);
                let host = Pubkey::create_program_address(&with_bump, &program.0);
                let expected = host.as_ref().copied().unwrap_or_default();
                let result = runner.run(
                    templates[index],
                    program,
                    &bytes_seed_inputs(&seeds, &expected, Some(bump as u64)),
                );
                match host {
                    Ok(_) => {
                        assert!(
                            result.program_result.is_ok(),
                            "{count} seeds, case {case}, bump {bump}: {:?}",
                            result.program_result
                        );
                        accepted += 1;
                    }
                    Err(_) => {
                        assert_eq!(
                            error_kind(&result),
                            Some(INVALID_PDA_DERIVATION),
                            "{count} seeds, case {case}, on-curve bump {bump}: {result:?}"
                        );
                        on_curve += 1;
                    }
                }
            }
            // A bump that does not fit in a byte is rejected before any derivation.
            if case % 6 == 0 {
                for bump in [256, u64::MAX] {
                    let result = runner.run(
                        templates[index],
                        program,
                        &bytes_seed_inputs(&seeds, &Pubkey::default(), Some(bump)),
                    );
                    assert_eq!(
                        error_kind(&result),
                        Some(INVALID_PDA_DERIVATION),
                        "{result:?}"
                    );
                    oversized += 1;
                }
            }
        }
    }
    eprintln!(
        "CREATE_PDA: {} runs over 1-15 seeds: {accepted} derived, {on_curve} on-curve rejections, \
         {oversized} oversized bumps",
        accepted + on_curve + oversized
    );
    assert!(on_curve > 100, "plenty of on-curve bumps: {on_curve}");
}

/// Fifteen 32-byte seeds, the most a template can derive from, for both opcodes.
#[test]
fn the_largest_seed_list_matches() {
    let mut rng = SplitMix(15);
    let programs = programs(&mut rng, 2);
    let mut runner = Runner::new();
    let search = runner.add(&bytes_seed_template(MAX_PDA_SEEDS, false));
    let supplied = runner.add(&bytes_seed_template(MAX_PDA_SEEDS, true));
    for program in &programs {
        for _ in 0..8 {
            let seeds: Vec<Vec<u8>> = (0..MAX_PDA_SEEDS)
                .map(|_| rng.bytes(MAX_PDA_SEED_LEN))
                .collect();
            let (expected, bump) = Pubkey::find_program_address(&slices(&seeds), &program.0);
            let result = runner.run(search, program, &bytes_seed_inputs(&seeds, &expected, None));
            assert!(result.program_result.is_ok(), "{:?}", result.program_result);
            let result = runner.run(
                supplied,
                program,
                &bytes_seed_inputs(&seeds, &expected, Some(bump as u64)),
            );
            assert!(result.program_result.is_ok(), "{:?}", result.program_result);
        }
    }
}

/// Seeds of every encoding a template can use, mixed in one derivation: a literal, a pubkey, and
/// integers of four widths, next to a bytes input.
#[test]
fn mixed_seed_encodings_match() {
    let mut builder = ProgramBuilder::new();
    let program = builder.account(ACCOUNT_EXECUTABLE, None, None, 0);
    let tag = builder.blob(b"vault");
    let owner_input = builder.input(VALUE_PUBKEY, 0);
    let index_input = builder.input(VALUE_U64, 0);
    let small_input = builder.input(VALUE_U64, 0);
    let note_input = builder.input(VALUE_BYTES, 20);
    let expected_input = builder.input(VALUE_PUBKEY, 0);
    let owner = builder.load_input(owner_input);
    let index = builder.load_input(index_input);
    let small = builder.load_input(small_input);
    let note = builder.load_input(note_input);
    let expected = builder.load_input(expected_input);
    let derived = builder.derive_pda(
        program,
        &[
            Segment::Literal(tag),
            Segment::Register(DATA_REG_PUBKEY, owner),
            Segment::Register(DATA_REG_U64, index),
            Segment::Register(DATA_REG_U16, small),
            Segment::Register(DATA_REG_U8, small),
            Segment::Register(DATA_REG_BYTES, note),
        ],
    );
    let same = builder.binary(OP_EQ, derived, expected);
    builder.require(same);
    let mut runner = Runner::new();
    let template = runner.add(&builder.build().expect("builds"));

    let mut rng = SplitMix(0x5eed);
    let programs = programs(&mut rng, 4);
    for case in 0..60 {
        let owner = rng.pubkey();
        let index = rng.next();
        let small = rng.below(256);
        let note_len = rng.below(21) as usize;
        let note = rng.bytes(note_len);
        let program = &programs[case % programs.len()];
        let (expected, _) = Pubkey::find_program_address(
            &[
                b"vault",
                owner.as_ref(),
                &index.to_le_bytes(),
                &(small as u16).to_le_bytes(),
                &[small as u8],
                &note,
            ],
            &program.0,
        );
        let mut inputs = owner.to_bytes().to_vec();
        inputs.extend_from_slice(&index.to_le_bytes());
        inputs.extend_from_slice(&small.to_le_bytes());
        inputs.extend_from_slice(&(note.len() as u16).to_le_bytes());
        inputs.extend_from_slice(&note);
        inputs.extend_from_slice(expected.as_ref());
        let result = runner.run(template, program, &inputs);
        assert!(
            result.program_result.is_ok(),
            "case {case}: {:?}",
            result.program_result
        );
    }
}

/// What a canonical search costs per attempt: one seed of 32 bytes, searches one to ten attempts
/// deep, and the least-squares slope of compute units against depth.
#[test]
fn derive_pda_cost_per_attempt() {
    let mut rng = SplitMix(0xc057);
    let program = mollusk_svm::program::keyed_account_for_system_program();
    let mut runner = Runner::new();
    let template = runner.add(&bytes_seed_template(1, false));
    let supplied = runner.add(&bytes_seed_template(1, true));

    let mut by_depth: Vec<(u32, u64)> = Vec::new();
    let mut wanted: Vec<u32> = (1..=10).collect();
    while !wanted.is_empty() {
        let seeds = vec![rng.bytes(32)];
        let (expected, bump) = Pubkey::find_program_address(&slices(&seeds), &program.0);
        let Some(position) = wanted.iter().position(|d| *d == depth(bump)) else {
            continue;
        };
        wanted.remove(position);
        let result = runner.run(
            template,
            &program,
            &bytes_seed_inputs(&seeds, &expected, None),
        );
        assert!(result.program_result.is_ok(), "{:?}", result.program_result);
        by_depth.push((depth(bump), result.compute_units_consumed));
        if depth(bump) == 1 {
            let result = runner.run(
                supplied,
                &program,
                &bytes_seed_inputs(&seeds, &expected, Some(bump as u64)),
            );
            assert!(result.program_result.is_ok(), "{:?}", result.program_result);
            eprintln!(
                "one 32-byte seed, bump supplied: {} CU",
                result.compute_units_consumed
            );
        }
    }
    by_depth.sort();
    let n = by_depth.len() as f64;
    let mean_x = by_depth.iter().map(|(d, _)| *d as f64).sum::<f64>() / n;
    let mean_y = by_depth.iter().map(|(_, cu)| *cu as f64).sum::<f64>() / n;
    let slope = by_depth
        .iter()
        .map(|(d, cu)| (*d as f64 - mean_x) * (*cu as f64 - mean_y))
        .sum::<f64>()
        / by_depth
            .iter()
            .map(|(d, _)| (*d as f64 - mean_x).powi(2))
            .sum::<f64>();
    eprintln!(
        "one 32-byte seed, canonical search, CU by attempts: {by_depth:?}; {slope:.1} CU per \
         attempt"
    );
    // sol_sha256 over 86 bytes (85 + 43) and sol_curve_validate_point (159), plus the loop.
    assert!(slope < 320.0, "a search attempt costs {slope:.1} CU");
}
