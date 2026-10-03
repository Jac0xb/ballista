//! Stateful lifecycle fuzzing: random sequences of create, begin, write-chunk, finalize, cancel
//! and run, from random signers, against a golden model of each template account's state. Every
//! step's success or failure (and, for the lifecycle's safety properties, its exact error code) is
//! checked against the model, so a step that should fail but succeeds — or vice versa — is caught.

use std::collections::HashMap;

use ballista_common::instruction::{
    IX_BEGIN_TEMPLATE, IX_CANCEL_TEMPLATE, IX_FINALIZE_TEMPLATE, IX_WRITE_TEMPLATE_CHUNK,
};
use ballista_common::template::ProgramBuilder;
use ballista_fuzz_gen::source::{Gen, SplitMix64};
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;
use solana_sdk_ids::system_program;

use super::harness::{Harness, BALLISTA_ID};

const INVALID_TEMPLATE_ACCOUNT: u32 = 6001;
const TEMPLATE_NOT_UPLOADING: u32 = 6003;
const TEMPLATE_NOT_FINALIZED: u32 = 6004;
const INVALID_CHUNK_OFFSET: u32 = 6006;

/// The model's view of one template account.
#[derive(Clone, Debug, PartialEq)]
enum State {
    Absent,
    Uploading { len: usize, written: usize, hash: [u8; 32], payload: Vec<u8> },
    Finalized,
}

/// A good small template (a bare `require(true)`), and a payload that does not verify.
fn good_payload() -> Vec<u8> {
    let mut builder = ProgramBuilder::new();
    let flag = builder.const_bool(true);
    builder.require(flag);
    builder.build().unwrap()
}

/// Bytes that parse as far as the header but fail verification (no instructions).
fn bad_payload() -> Vec<u8> {
    // A valid-looking payload with zero instructions fails with `TooManyInstructions`.
    let builder = ProgramBuilder::new();
    let mut bytes = builder.build().unwrap();
    // `build` with nothing still yields a header; ensure it is non-empty so parsing proceeds.
    if bytes.len() < 2 {
        bytes.push(0);
    }
    bytes
}

fn hash_of(payload: &[u8]) -> [u8; 32] {
    solana_sha256_hasher::hash(payload).to_bytes()
}

fn template_pda(creator: &Pubkey, id: u16) -> Pubkey {
    Pubkey::find_program_address(&[b"template", creator.as_ref(), &id.to_le_bytes()], &BALLISTA_ID).0
}

struct Lifecycle<'a> {
    harness: &'a Harness,
    creators: Vec<Pubkey>,
    ids: Vec<u16>,
    /// Model state keyed by (creator, id).
    state: HashMap<(Pubkey, u16), State>,
    failures: Vec<String>,
}

impl Lifecycle<'_> {
    fn key(&self, g: &mut Gen) -> (Pubkey, u16) {
        let creator = self.creators[g.below(self.creators.len())];
        let id = self.ids[g.below(self.ids.len())];
        (creator, id)
    }

    fn fund(&self, address: &Pubkey, lamports: u64) {
        self.harness
            .context
            .account_store
            .borrow_mut()
            .insert(*address, Account::new(lamports, 0, &system_program::id()));
    }

    fn template_account(&self, template: &Pubkey) -> Option<Account> {
        self.harness.context.account_store.borrow().get(template).cloned()
    }

    fn code(result: &mollusk_svm::result::types::InstructionResult) -> Option<u32> {
        match &result.program_result {
            mollusk_svm::result::types::ProgramResult::Failure(
                solana_program_error::ProgramError::Custom(code),
            ) => Some(*code & 0xffff),
            _ => None,
        }
    }

    /// Runs one random step and checks it against the model.
    fn step(&mut self, g: &mut Gen) {
        match g.below(6) {
            0 => self.create(g),
            1 => self.begin(g),
            2 => self.write(g),
            3 => self.finalize(g),
            4 => self.cancel(g),
            _ => self.run(g),
        }
    }

    fn instruction(&self, data: Vec<u8>, signer: Pubkey, template: Pubkey, system: bool) -> Instruction {
        let mut accounts = vec![AccountMeta::new(signer, true), AccountMeta::new(template, false)];
        if system {
            accounts.push(AccountMeta::new_readonly(system_program::id(), false));
        }
        Instruction { program_id: BALLISTA_ID, accounts, data }
    }

    fn expect(&mut self, label: &str, key: (Pubkey, u16), ok: bool, result_ok: bool, code: Option<u32>, want_code: Option<u32>) {
        if ok != result_ok {
            self.failures.push(format!(
                "{label} on {:?}: model expected {}, program {} (code {:?})",
                key,
                if ok { "success" } else { "failure" },
                if result_ok { "succeeded" } else { "failed" },
                code
            ));
        } else if !ok {
            if let Some(want) = want_code {
                if code != Some(want) {
                    self.failures.push(format!("{label} on {:?}: expected code {want}, got {:?}", key, code));
                }
            }
        }
    }

    fn create(&mut self, g: &mut Gen) {
        let (creator, id) = self.key(g);
        let template = template_pda(&creator, id);
        let good = g.chance(4, 5);
        let payload = if good { good_payload() } else { bad_payload() };
        let hash = hash_of(&payload);
        let mut data = vec![ballista_common::instruction::IX_CREATE_TEMPLATE];
        data.extend_from_slice(&id.to_le_bytes());
        data.extend_from_slice(&hash);
        data.extend_from_slice(&payload);
        self.fund(&creator, 10_000_000_000);
        let state = self.state.entry((creator, id)).or_insert(State::Absent).clone();
        // Create succeeds only on an absent account with a verifying payload.
        let ok = state == State::Absent && good;
        let result = self.harness.context.process_instruction(&self.instruction(data, creator, template, true));
        self.expect("create", (creator, id), ok, result.program_result.is_ok(), Self::code(&result), None);
        if result.program_result.is_ok() {
            self.state.insert((creator, id), State::Finalized);
            // The template must sit at its derived PDA and be owned by Ballista.
            if let Some(account) = self.template_account(&template) {
                if account.owner != BALLISTA_ID {
                    self.failures.push(format!("create: template {template} not owned by Ballista"));
                }
            }
        }
    }

    fn begin(&mut self, g: &mut Gen) {
        let (creator, id) = self.key(g);
        let template = template_pda(&creator, id);
        let payload = good_payload();
        let len = payload.len();
        let hash = hash_of(&payload);
        let mut data = vec![IX_BEGIN_TEMPLATE];
        data.extend_from_slice(&id.to_le_bytes());
        data.extend_from_slice(&(len as u32).to_le_bytes());
        data.extend_from_slice(&hash);
        self.fund(&creator, 10_000_000_000);
        let state = self.state.entry((creator, id)).or_insert(State::Absent).clone();
        let ok = state == State::Absent;
        let result = self.harness.context.process_instruction(&self.instruction(data, creator, template, true));
        self.expect("begin", (creator, id), ok, result.program_result.is_ok(), Self::code(&result), None);
        if result.program_result.is_ok() {
            self.state.insert((creator, id), State::Uploading { len, written: 0, hash, payload });
        }
    }

    fn write(&mut self, g: &mut Gen) {
        let (creator, id) = self.key(g);
        let template = template_pda(&creator, id);
        // Sometimes a non-creator signs the write.
        let signer = if g.chance(1, 4) { self.creators[g.below(self.creators.len())] } else { creator };
        self.fund(&signer, 10_000_000_000);
        let state = self.state.entry((creator, id)).or_insert(State::Absent).clone();
        let (chunk, offset, want_code, ok) = match &state {
            State::Uploading { len, written, payload, .. } => {
                // Mostly the correct next chunk; sometimes a wrong offset.
                let wrong_offset = g.chance(1, 5);
                let offset = if wrong_offset { (*written + 1).min(*len) } else { *written };
                let remaining = len.saturating_sub(offset);
                let take = remaining.min(1 + g.below(remaining.max(1)));
                let chunk = payload.get(offset..offset + take).unwrap_or(&[]).to_vec();
                let creator_ok = signer == creator;
                let offset_ok = offset == *written && !chunk.is_empty();
                let want = if !creator_ok {
                    None // a non-creator either fails the signer check or the creator/PDA check
                } else if !offset_ok {
                    Some(INVALID_CHUNK_OFFSET)
                } else {
                    None
                };
                (chunk, offset, want, creator_ok && offset_ok)
            }
            // Write on absent/finalized fails.
            State::Absent => (vec![0u8], 0usize, Some(INVALID_TEMPLATE_ACCOUNT), false),
            State::Finalized => (vec![0u8], 0usize, Some(TEMPLATE_NOT_UPLOADING), false),
        };
        if chunk.is_empty() {
            return;
        }
        let mut data = vec![IX_WRITE_TEMPLATE_CHUNK];
        data.extend_from_slice(&(offset as u32).to_le_bytes());
        data.extend_from_slice(&chunk);
        let result = self.harness.context.process_instruction(&self.instruction(data, signer, template, false));
        let want = if signer == creator { want_code } else { None };
        self.expect("write", (creator, id), ok, result.program_result.is_ok(), Self::code(&result), want);
        if result.program_result.is_ok() {
            if let State::Uploading { written, .. } = self.state.get_mut(&(creator, id)).unwrap() {
                *written = offset + chunk.len();
            }
        }
    }

    fn finalize(&mut self, g: &mut Gen) {
        let (creator, id) = self.key(g);
        let template = template_pda(&creator, id);
        let signer = if g.chance(1, 4) { self.creators[g.below(self.creators.len())] } else { creator };
        self.fund(&signer, 10_000_000_000);
        let state = self.state.entry((creator, id)).or_insert(State::Absent).clone();
        let (ok, want) = match &state {
            State::Uploading { len, written, .. } => {
                let complete = written == len;
                let creator_ok = signer == creator;
                let code = if !creator_ok {
                    None
                } else if !complete {
                    Some(INVALID_CHUNK_OFFSET)
                } else {
                    None // complete good payload verifies
                };
                (creator_ok && complete, code)
            }
            State::Absent => (false, Some(INVALID_TEMPLATE_ACCOUNT)),
            State::Finalized => (false, Some(TEMPLATE_NOT_UPLOADING)),
        };
        let result = self.harness.context.process_instruction(&self.instruction(vec![IX_FINALIZE_TEMPLATE], signer, template, false));
        let want = if signer == creator { want } else { None };
        self.expect("finalize", (creator, id), ok, result.program_result.is_ok(), Self::code(&result), want);
        if result.program_result.is_ok() {
            self.state.insert((creator, id), State::Finalized);
        }
    }

    fn cancel(&mut self, g: &mut Gen) {
        let (creator, id) = self.key(g);
        let template = template_pda(&creator, id);
        let signer = if g.chance(1, 4) { self.creators[g.below(self.creators.len())] } else { creator };
        self.fund(&signer, 10_000_000_000);
        let state = self.state.entry((creator, id)).or_insert(State::Absent).clone();
        let (ok, want) = match &state {
            State::Uploading { .. } => (signer == creator, None),
            State::Absent => (false, Some(INVALID_TEMPLATE_ACCOUNT)),
            // A finalized template can never be cancelled.
            State::Finalized => (false, Some(TEMPLATE_NOT_UPLOADING)),
        };
        let creator_before = self.template_account(&creator).map(|a| a.lamports).unwrap_or(0);
        let template_before = self.template_account(&template).map(|a| a.lamports).unwrap_or(0);
        let result = self.harness.context.process_instruction(&self.instruction(vec![IX_CANCEL_TEMPLATE], signer, template, false));
        let want = if signer == creator { want } else { None };
        self.expect("cancel", (creator, id), ok, result.program_result.is_ok(), Self::code(&result), want);
        if result.program_result.is_ok() {
            self.state.insert((creator, id), State::Absent);
            // The rent returns to the creator, and the template account is emptied.
            let creator_after = self.template_account(&creator).map(|a| a.lamports).unwrap_or(0);
            let template_after = self.template_account(&template).map(|a| a.lamports).unwrap_or(0);
            if creator_after != creator_before + template_before {
                self.failures.push(format!("cancel: rent not returned to creator ({creator_before}+{template_before} != {creator_after})"));
            }
            if template_after != 0 {
                self.failures.push(format!("cancel: template still holds {template_after} lamports"));
            }
        }
    }

    fn run(&mut self, g: &mut Gen) {
        let (creator, id) = self.key(g);
        let template = template_pda(&creator, id);
        let state = self.state.entry((creator, id)).or_insert(State::Absent).clone();
        // The trivial template has no account groups and no inputs, so the run data is empty.
        let instruction = Instruction {
            program_id: BALLISTA_ID,
            accounts: vec![AccountMeta::new_readonly(template, false)],
            data: vec![ballista_common::instruction::IX_RUN],
        };
        let result = self.harness.context.process_instruction(&instruction);
        // A run needs a finalized template; anything else fails (not finalized, or no account).
        let ok = state == State::Finalized;
        let want = match state {
            State::Finalized => None,
            State::Uploading { .. } => Some(TEMPLATE_NOT_FINALIZED),
            State::Absent => Some(INVALID_TEMPLATE_ACCOUNT),
        };
        self.expect("run", (creator, id), ok, result.program_result.is_ok(), Self::code(&result), want);
    }
}

#[test]
fn fuzz_lifecycle_sequences() {
    let harness = Harness::new();
    let cases: u64 = std::env::var("FV_LIFECYCLE_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(400);
    let steps: usize = std::env::var("FV_LIFECYCLE_STEPS").ok().and_then(|v| v.parse().ok()).unwrap_or(16);
    let creators: Vec<Pubkey> = (0..3).map(|n| Pubkey::new_from_array([0x30 + n; 32])).collect();
    let ids = vec![1u16, 2];

    let mut all_failures = Vec::new();
    for seed in 0..cases {
        harness.reset();
        let mut source = SplitMix64::new(seed ^ 0xA5A5_0000);
        let mut g = Gen::new(&mut source);
        let mut lifecycle = Lifecycle {
            harness: &harness,
            creators: creators.clone(),
            ids: ids.clone(),
            state: HashMap::new(),
            failures: Vec::new(),
        };
        for _ in 0..steps {
            lifecycle.step(&mut g);
        }
        for failure in lifecycle.failures {
            all_failures.push(format!("seed {seed}: {failure}"));
        }
        if all_failures.len() > 30 {
            break;
        }
    }
    assert!(all_failures.is_empty(), "lifecycle model mismatches:\n{}", all_failures.join("\n"));
}
