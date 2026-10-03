//! The wire format (`common/src/template/wire.rs`, `account.rs`, `instruction.rs`): the parser,
//! the run path's `parse_finalized` and `finalized_program_unchecked`, the template account's
//! readers, the instruction-data parser, and the reference accessors the verifier and the executor
//! share.
//!
//! The parser proofs take every payload up to the 10,240-byte cap, the largest a template can hold
//! (the bound the swarm's critic showed feasible). They are cheap at that size because the parser
//! reads only the 24 header bytes and slice bounds.

use core::mem::size_of;

use ballista_common::template::*;

/// `data` is a symbolic-length prefix of `bytes`: every length from 0 to `N`.
fn any_prefix<const N: usize>(bytes: &[u8; N]) -> &[u8] {
    let len: usize = kani::any_where(|len: &usize| *len <= N);
    &bytes[..len]
}

/// Every section of `program` sits inside `data`, in declaration order, back to back, with the
/// length its header count gives, and the blob ends exactly at the end of `data`.
fn assert_sections_tile(data: &[u8], program: &ProgramView<'_>) {
    let header = program.header;
    let base = data.as_ptr();
    assert!(core::ptr::eq(header as *const ProgramHeader as *const u8, base));
    // Each section starts where the previous one ended; the first right after the header.
    let mut at = PROGRAM_HEADER_LEN;
    macro_rules! section {
        ($slice:expr, $count:expr, $record:ty) => {{
            assert_eq!($slice.len(), $count);
            assert!(core::ptr::eq($slice.as_ptr() as *const u8, base.wrapping_add(at)));
            at += $slice.len() * size_of::<$record>();
        }};
    }
    section!(program.accounts, header.fixed_account_count() + header.batch_stride(), AccountConstraint);
    section!(program.inputs, header.total_input_count(), InputDescriptor);
    section!(program.instructions, header.instruction_count(), InstructionRecord);
    section!(program.cpis, header.cpi_count(), CpiDescriptor);
    section!(program.cpi_accounts, header.cpi_account_count(), CpiAccountRecord);
    section!(program.data_segments, header.data_segment_count(), DataSegment);
    section!(program.pubkeys, header.pubkey_count(), PubkeyRecord);
    section!(program.blob, header.blob_len(), u8);
    assert_eq!(at, data.len());
}

/// The two views name the same bytes for every section.
fn assert_same_view(a: &ProgramView<'_>, b: &ProgramView<'_>) {
    assert!(core::ptr::eq(a.header, b.header));
    assert!(core::ptr::eq(a.accounts, b.accounts));
    assert!(core::ptr::eq(a.inputs, b.inputs));
    assert!(core::ptr::eq(a.instructions, b.instructions));
    assert!(core::ptr::eq(a.cpis, b.cpis));
    assert!(core::ptr::eq(a.cpi_accounts, b.cpi_accounts));
    assert!(core::ptr::eq(a.data_segments, b.data_segments));
    assert!(core::ptr::eq(a.pubkeys, b.pubkeys));
    assert!(core::ptr::eq(a.blob, b.blob));
}

/// `ProgramView::parse` never panics or reads out of bounds, on every payload of up to 10,240
/// bytes (the cap). When it accepts one, the magic, version, flags and reserved byte are valid and
/// the nine sections tile the payload exactly: each has its header count's length, starts where
/// the previous one ended, and the blob ends at the payload's end. A payload shorter than the
/// header is `Truncated`; a foreign magic is reported before the version, and the version before
/// anything else. Bound: payload length 0 to 10,240, every byte symbolic.
#[kani::proof]
#[kani::unwind(5)]
fn parse_never_panics_and_sections_tile_the_payload() {
    let bytes: [u8; MAX_TEMPLATE_PAYLOAD_LEN] = kani::any();
    let data = any_prefix(&bytes);
    let short = data.len() < PROGRAM_HEADER_LEN;
    let magic_ok = !short && data[..4] == TEMPLATE_PROGRAM_MAGIC;
    let version_ok = !short && data[4] == TEMPLATE_PROGRAM_VERSION;
    match ProgramView::parse(data) {
        Ok(program) => {
            assert!(magic_ok && version_ok);
            assert_eq!(program.header.flags() & !PROGRAM_FLAGS_MASK, 0);
            assert_eq!(program.header.reserved(), &[0]);
            assert_sections_tile(data, &program);
            kani::cover!(program.instructions.len() == MAX_VM_INSTRUCTIONS, "a 128-instruction program parses");
            kani::cover!(!program.pubkeys.is_empty() && !program.blob.is_empty(), "every trailing section non-empty");
        }
        Err(error) => {
            kani::cover!(error == TemplateError::SectionLengthMismatch, "a section length mismatch");
            kani::cover!(error == TemplateError::InvalidReservedBytes, "bad flags or reserved byte");
            kani::cover!(!short && error == TemplateError::Truncated, "a truncated section");
            let expected_first = if short {
                Some(TemplateError::Truncated)
            } else if !magic_ok {
                Some(TemplateError::InvalidMagic)
            } else if !version_ok {
                Some(TemplateError::UnsupportedVersion(data[4]))
            } else {
                None
            };
            match expected_first {
                Some(expected) => assert_eq!(error, expected),
                None => assert!(matches!(
                    error,
                    TemplateError::InvalidReservedBytes
                        | TemplateError::Truncated
                        | TemplateError::SectionLengthMismatch
                )),
            }
        }
    }
}

/// A payload past the 10,240-byte cap is refused with `PayloadTooLarge` and its length, before
/// anything is read. Bound: lengths 10,241 to 10,248, every byte symbolic.
#[kani::proof]
fn parse_refuses_payloads_over_the_cap() {
    let bytes: [u8; MAX_TEMPLATE_PAYLOAD_LEN + 8] = kani::any();
    let len: usize = kani::any_where(|len: &usize| *len > MAX_TEMPLATE_PAYLOAD_LEN && *len <= bytes.len());
    let refused = matches!(ProgramView::parse(&bytes[..len]), Err(TemplateError::PayloadTooLarge(n)) if n == len);
    assert!(refused);
    kani::cover!(len == MAX_TEMPLATE_PAYLOAD_LEN + 1, "one byte over the cap");
}

/// The run path's `parse_finalized` agrees with `parse` on every payload up to the cap: on every
/// payload `parse` accepts it returns the same view, all nine sections naming the same bytes; and
/// it accepts a payload `parse` refuses only for a check it documents skipping (flags and the
/// reserved byte; the cap is outside this bound and `parse_refuses_payloads_over_the_cap` covers
/// it), still tiling the payload exactly. Bound: payload length 0 to 10,240, every byte symbolic.
#[kani::proof]
#[kani::unwind(5)]
fn parse_finalized_agrees_with_parse() {
    let bytes: [u8; MAX_TEMPLATE_PAYLOAD_LEN] = kani::any();
    let data = any_prefix(&bytes);
    let full = ProgramView::parse(data);
    let fast = ProgramView::parse_finalized(data);
    assert!(full.is_err() || fast.is_some(), "parse_finalized refused a payload parse accepts");
    if let (Ok(full), Some(fast)) = (&full, &fast) {
        assert_same_view(full, fast);
        kani::cover!(full.instructions.len() > 1, "agreement on a multi-instruction program");
    }
    if let (Err(error), Some(fast)) = (&full, &fast) {
        assert_eq!(*error, TemplateError::InvalidReservedBytes);
        assert_sections_tile(data, fast);
        kani::cover!(fast.instructions.len() > 0, "parse_finalized accepts what only the reserved checks refuse");
    }
    kani::cover!(full.is_err() && fast.is_none(), "both refuse");
}

/// Bytes after the 80-byte template account header in the account-level proofs: the cap.
const ACCOUNT_PAYLOAD: usize = MAX_TEMPLATE_PAYLOAD_LEN;

/// A run reads its template through `finalized_program_unchecked`, which skips most of
/// `TemplateAccount::parse`. On every account the full path (`parse` then `finalized_program`)
/// accepts, the fast path returns the same program; whenever the fast path returns a program, its
/// sections tile the bytes after the header. (The fast path also accepts finalized accounts the
/// full path refuses, such as one with nonzero reserved bytes: only Ballista writes a finalized
/// template, after `parse` and `verify` accepted it, so a run never meets one.) Bound: account data
/// 0 to 10,320 bytes (the header plus the cap), every byte symbolic.
#[kani::proof]
#[kani::unwind(5)]
fn finalized_fast_path_agrees_with_the_full_parse() {
    let bytes: [u8; TEMPLATE_ACCOUNT_HEADER_LEN + ACCOUNT_PAYLOAD] = kani::any();
    let data = any_prefix(&bytes);
    let full = TemplateAccount::parse(data).and_then(|account| account.finalized_program());
    let fast = TemplateAccount::finalized_program_unchecked(data);
    assert!(full.is_err() || fast.is_some(), "the fast path refused a template the full path accepts");
    if let (Ok(full), Some(fast)) = (&full, &fast) {
        assert_same_view(full, fast);
        kani::cover!(full.instructions.len() > 1, "agreement on a multi-instruction template");
    }
    if let Some(fast) = &fast {
        assert_sections_tile(&data[TEMPLATE_ACCOUNT_HEADER_LEN..], fast);
    }
    kani::cover!(full.is_err() && fast.is_some(), "the fast path accepts what the full path refuses");
}

/// `TemplateAccount::parse` and `split_template_account_mut` never panic, and both return a
/// payload exactly as long as the header declares, directly after the 80-byte header; `parse`
/// also guarantees `written_len ≤ payload_len`, with equality once finalized. Bound: account data
/// 0 to 10,320 bytes, every byte symbolic.
#[kani::proof]
#[kani::unwind(5)]
fn template_account_split_is_exact() {
    let mut bytes: [u8; TEMPLATE_ACCOUNT_HEADER_LEN + ACCOUNT_PAYLOAD] = kani::any();
    let len: usize = kani::any_where(|len: &usize| *len <= TEMPLATE_ACCOUNT_HEADER_LEN + ACCOUNT_PAYLOAD);
    let base = bytes.as_ptr();
    if let Ok(account) = TemplateAccount::parse(&bytes[..len]) {
        let header = account.header();
        assert_eq!(account.payload().len(), header.payload_len());
        assert!(header.written_len() <= header.payload_len());
        assert!(!header.is_finalized() || header.written_len() == header.payload_len());
        assert!(core::ptr::eq(account.payload().as_ptr(), base.wrapping_add(TEMPLATE_ACCOUNT_HEADER_LEN)));
        kani::cover!(header.is_finalized(), "a finalized account parses");
        kani::cover!(header.written_len() < header.payload_len(), "a partly written upload parses");
    }
    if let Ok((header, payload)) = split_template_account_mut(&mut bytes[..len]) {
        assert_eq!(payload.len(), header.payload_len());
        assert!(core::ptr::eq(payload.as_ptr(), base.wrapping_add(TEMPLATE_ACCOUNT_HEADER_LEN)));
        assert_eq!(TEMPLATE_ACCOUNT_HEADER_LEN + payload.len(), len);
        kani::cover!(payload.len() == ACCOUNT_PAYLOAD, "a full-size payload splits");
    }
}

/// A `ProgramView` over symbolic sections, built directly rather than parsed: up to 4 fixed
/// accounts, a batch stride up to 4, and up to 4 fixed and 4 row inputs.
struct Shape {
    header: ProgramHeader,
    accounts: [AccountConstraint; 8],
    inputs: [InputDescriptor; 8],
}

impl Shape {
    fn any() -> Self {
        let fixed: u8 = kani::any_where(|n: &u8| *n <= 4);
        let stride: u8 = kani::any_where(|n: &u8| *n <= 4);
        let inputs: u8 = kani::any_where(|n: &u8| *n <= 4);
        let row_inputs: u8 = kani::any_where(|n: &u8| *n <= 4);
        let header = ProgramHeader::new(
            fixed, stride, kani::any(), 0, inputs, 0, 0, 0, 0, 0, 0, 0, 0, row_inputs, 0,
        );
        Self {
            header,
            accounts: core::array::from_fn(|_| crate::util::any::constraint()),
            inputs: core::array::from_fn(|_| crate::util::any::input()),
        }
    }

    fn view(&self) -> ProgramView<'_> {
        let accounts = self.header.fixed_account_count() + self.header.batch_stride();
        ProgramView {
            header: &self.header,
            accounts: &self.accounts[..accounts],
            inputs: &self.inputs[..self.header.total_input_count()],
            instructions: &[],
            cpis: &[],
            cpi_accounts: &[],
            data_segments: &[],
            pubkeys: &[],
            blob: &[],
        }
    }
}

/// `account_constraint` and `input_descriptor` never panic, and name exactly: for a fixed
/// reference below the fixed count, its own record; for a row reference (`0x80 | offset`) inside a
/// row loop with the offset below the stride or row input count, record `fixed + offset`;
/// otherwise nothing. Bound: every reference byte, both scopes, up to 4 fixed and 4 row accounts
/// and inputs.
#[kani::proof]
#[kani::unwind(9)]
fn reference_accessors_name_exactly_their_record() {
    let shape = Shape::any();
    let program = shape.view();
    let reference: u8 = kani::any();
    let in_row_loop: bool = kani::any();
    let row = reference & ITERATION_ACCOUNT_BIT != 0;
    let offset = (reference & !ITERATION_ACCOUNT_BIT) as usize;

    let fixed = program.header.fixed_account_count();
    let expected_account = if !row {
        (offset < fixed).then_some(offset)
    } else if in_row_loop && offset < program.header.batch_stride() {
        Some(fixed + offset)
    } else {
        None
    };
    let found = program.account_constraint(reference, in_row_loop);
    assert_eq!(found.is_some(), expected_account.is_some());
    if let (Some(found), Some(index)) = (found, expected_account) {
        assert!(core::ptr::eq(found, &program.accounts[index]));
        kani::cover!(row, "a row account resolves");
        kani::cover!(!row, "a fixed account resolves");
    }

    let fixed_inputs = program.header.input_count();
    let expected_input = if !row {
        (offset < fixed_inputs).then_some(offset)
    } else if in_row_loop && offset < program.header.row_input_count() {
        Some(fixed_inputs + offset)
    } else {
        None
    };
    let found = program.input_descriptor(reference, in_row_loop);
    assert_eq!(found.is_some(), expected_input.is_some());
    if let (Some(found), Some(index)) = (found, expected_input) {
        assert!(core::ptr::eq(found, &program.inputs[index]));
        kani::cover!(row, "a row input resolves");
    }
    kani::cover!(row && !in_row_loop, "a row reference outside a row loop");
}

/// Instruction data bytes the instruction parser proof explores: the longest fixed layout is
/// `BeginTemplate`'s 39 bytes.
const INSTRUCTION_DATA: usize = 48;

/// `BallistaInstruction::parse` never panics, and every instruction it accepts is exactly its
/// documented layout: re-encoding the parsed fields gives back the input byte for byte, so no
/// trailing or missing byte is ignored and no two inputs parse alike. `CreateTemplate` is
/// `[0] ‖ id (u16) ‖ hash (32) ‖ payload (1..=10,240)`, `BeginTemplate` `[1] ‖ id ‖ length (u32,
/// 1..=10,240) ‖ hash`, `WriteTemplateChunk` `[2] ‖ offset (u32) ‖ bytes (1..=10,240)`,
/// `FinalizeTemplate` `[3]`, `CancelTemplate` `[4]`, `Run` `[5] ‖ inputs (at most 1,024)`.
/// Bound: instruction data 0 to 48 bytes, every byte symbolic (so the payload, chunk and input
/// length caps themselves are not reached).
#[kani::proof]
#[kani::unwind(49)]
fn instruction_data_parses_exactly_its_layout() {
    use ballista_common::instruction::BallistaInstruction;
    let bytes: [u8; INSTRUCTION_DATA] = kani::any();
    let data = any_prefix(&bytes);
    let Ok(instruction) = BallistaInstruction::parse(data) else {
        kani::cover!(data.len() == 1 && data[0] > 5, "an unknown discriminator is refused");
        return;
    };
    let mut encoded = [0u8; INSTRUCTION_DATA];
    let mut at = 0;
    let mut put = |part: &[u8]| {
        encoded[at..at + part.len()].copy_from_slice(part);
        at += part.len();
    };
    match instruction {
        BallistaInstruction::CreateTemplate { template_id, payload_hash, payload } => {
            assert!(!payload.is_empty());
            put(&[0]);
            put(&template_id.to_le_bytes());
            put(payload_hash);
            put(payload);
            kani::cover!(true, "CreateTemplate");
        }
        BallistaInstruction::BeginTemplate { template_id, payload_len, payload_hash } => {
            assert!((1..=MAX_TEMPLATE_PAYLOAD_LEN).contains(&payload_len));
            put(&[1]);
            put(&template_id.to_le_bytes());
            put(&(payload_len as u32).to_le_bytes());
            put(payload_hash);
            kani::cover!(true, "BeginTemplate");
        }
        BallistaInstruction::WriteTemplateChunk { offset, bytes } => {
            assert!(!bytes.is_empty());
            put(&[2]);
            put(&(offset as u32).to_le_bytes());
            put(bytes);
            kani::cover!(true, "WriteTemplateChunk");
        }
        BallistaInstruction::FinalizeTemplate => put(&[3]),
        BallistaInstruction::CancelTemplate => put(&[4]),
        BallistaInstruction::Run { input_bytes } => {
            put(&[5]);
            put(input_bytes);
            kani::cover!(!input_bytes.is_empty(), "Run with inputs");
        }
    }
    assert_eq!(at, data.len());
    for i in 0..data.len() {
        assert_eq!(encoded[i], data[i]);
    }
}
