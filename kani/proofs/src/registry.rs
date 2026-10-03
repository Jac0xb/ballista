//! Registry entries (`programs/ballista/src/processor/registry.rs`): the header an entry holds, the
//! check an open makes of an existing entry, and the field writes.
//!
//! The expected header is written out byte by byte from the layout the module documents (`BREG`,
//! version 1, the registry index, two zero bytes, the template's address, the key), not from
//! `EntryId::header`, so the open proof checks the code against the documented layout.

use ballista::error::BallistaError;
use ballista::processor::execute::{RunError, RuntimeValue};
use ballista::processor::registry::{open, write_field, EntryId};
use ballista_common::template::{
    split_template_account_mut, TemplateAccount, OP_READ_BOOL, OP_READ_I64, OP_READ_PUBKEY,
    OP_READ_U128, OP_READ_U64, REGISTRY_ENTRY_HEADER_LEN,
};
use pinocchio::account::NOT_BORROWED;
use pinocchio::Address;

use crate::accounts::{AccountMemory, Fields};
use crate::util::any_runtime_value;

const HEADER: usize = REGISTRY_ENTRY_HEADER_LEN;

/// Field bytes after the header in the entry proofs. The size an open names is any `u16`.
const FIELDS: usize = 16;

fn err(kind: BallistaError) -> RunError {
    RunError::Vm(kind)
}

/// The documented header, byte by byte.
fn documented_header(template: &[u8; 32], index: u8, key: &[u8; 32]) -> [u8; HEADER] {
    let mut header = [0u8; HEADER];
    header[0] = b'B';
    header[1] = b'R';
    header[2] = b'E';
    header[3] = b'G';
    header[4] = 1;
    header[5] = index;
    for i in 0..32 {
        header[8 + i] = template[i];
        header[40 + i] = key[i];
    }
    header
}

/// `EntryId::header` is the documented layout, and it identifies its entry: two ids give the same
/// header exactly when their template, registry index and key are all equal. So an open that
/// matches the header has found the entry it was asked for and no other. Unbounded: every
/// template address, index and key.
#[kani::proof]
#[kani::unwind(73)]
fn entry_headers_follow_the_layout_and_identify_their_entry() {
    let (template_a, template_b): ([u8; 32], [u8; 32]) = (kani::any(), kani::any());
    let (key_a, key_b): ([u8; 32], [u8; 32]) = (kani::any(), kani::any());
    let (index_a, index_b): (u8, u8) = (kani::any(), kani::any());
    let (address_a, address_b) = (Address::new_from_array(template_a), Address::new_from_array(template_b));
    let a = EntryId { template: &address_a, index: index_a, key: key_a }.header();
    let b = EntryId { template: &address_b, index: index_b, key: key_b }.header();
    assert!(a == documented_header(&template_a, index_a, &key_a));
    let same_id = template_a == template_b && index_a == index_b && key_a == key_b;
    assert_eq!(a == b, same_id);
    kani::cover!(!same_id && template_a == template_b && key_a == key_b, "ids differing only in the index");
    kani::cover!(same_id, "the same id twice");
}

/// An open of an entry Ballista already owns succeeds exactly when the account is writable, not
/// borrowed, exactly `72 + size` bytes long, and starts with the documented header for this
/// template, registry index and key; it then marks the entry's data exclusively borrowed. Every
/// other case fails with `InvalidRegistryEntry` and leaves the borrow state alone. Bounded: data
/// up to `72 + FIELDS` bytes; the size, the header bytes, the id, the writable flag and the borrow
/// state are all symbolic.
#[kani::proof]
#[kani::unwind(73)]
fn open_accepts_exactly_the_documented_header_and_size() {
    let template: [u8; 32] = kani::any();
    let key: [u8; 32] = kani::any();
    let index: u8 = kani::any();
    let size: u16 = kani::any();
    let data: [u8; HEADER + FIELDS] = kani::any();
    let data_len: usize = kani::any_where(|len: &usize| *len <= HEADER + FIELDS);
    let mut fields = Fields::plain(kani::any(), ballista::ID.to_bytes());
    fields.writable = kani::any();
    fields.borrow_state = kani::any();
    let (writable, borrow_state) = (fields.writable, fields.borrow_state);
    let mut entry = AccountMemory::new(fields, data, data_len);
    let mut payer = AccountMemory::new(Fields::plain([1; 32], [0; 32]), [0u8; 0], 0);

    let template_address = Address::new_from_array(template);
    let id = EntryId { template: &template_address, index, key };
    let result = open(&entry.view(), &payer.view(), &id, usize::from(size));

    let expected = documented_header(&template, index, &key);
    let header_matches = data_len == HEADER + usize::from(size) && {
        let mut same = true;
        for i in 0..HEADER {
            same &= data[i] == expected[i];
        }
        same
    };
    if writable && header_matches && borrow_state == NOT_BORROWED {
        assert_eq!(result, Ok(()));
        assert_eq!(entry.header.borrow_state, 0, "an open marks the entry");
        kani::cover!(size == FIELDS as u16, "an entry with every field byte opens");
    } else {
        assert_eq!(result, Err(err(BallistaError::InvalidRegistryEntry)));
        assert_eq!(entry.header.borrow_state, borrow_state);
        kani::cover!(writable && header_matches && borrow_state == 0, "an entry already open is refused");
        kani::cover!(writable && !header_matches && data_len == HEADER + usize::from(size), "a right-sized entry with another header");
    }
}

/// An open of an account Ballista does not own, that is not an empty System account, fails with
/// `InvalidRegistryEntry` before any derivation or CPI: one owned by another program, with any data
/// length up to 8 bytes, and a System account that holds data. (An empty System account goes on
/// to the creation path, which derives the entry's address and calls the System program; that is
/// out of reach here.) The owners are concrete: with a symbolic owner the solver would also unroll
/// the unreachable derivation, a SHA-256 per bump for up to 255 bumps. The open compares the owner
/// only with Ballista's id and the System program's, so one foreign owner stands for all. The id
/// and the size are symbolic.
#[kani::proof]
fn open_refuses_foreign_accounts_before_creating() {
    let template = Address::new_from_array(kani::any());
    let id = EntryId { template: &template, index: kani::any(), key: kani::any() };
    let size = usize::from(kani::any::<u16>());
    let mut payer = AccountMemory::new(Fields::plain([1; 32], [0; 32]), [0u8; 0], 0);
    {
        let mut fields = Fields::plain(kani::any(), [5; 32]);
        fields.writable = true;
        let len: usize = kani::any_where(|len: &usize| *len <= 8);
        let mut entry = AccountMemory::new(fields, kani::any::<[u8; 8]>(), len);
        assert_eq!(open(&entry.view(), &payer.view(), &id, size), Err(err(BallistaError::InvalidRegistryEntry)));
    }
    {
        let mut fields = Fields::plain(kani::any(), [0; 32]);
        fields.writable = true;
        let mut entry = AccountMemory::new(fields, kani::any::<[u8; 8]>(), 3);
        assert_eq!(open(&entry.view(), &payer.view(), &id, size), Err(err(BallistaError::InvalidRegistryEntry)));
    }
    kani::cover!(size == 0, "a zero-size open");
}

/// `write_field` writes exactly the bytes of its field: for a value whose type matches the
/// selector, the little-endian bytes at `offset..offset + width` when that range lies inside the
/// account's data, and nothing else, anywhere; otherwise it changes no byte and fails with
/// `InvalidTemplateProgram` (out of range), `InvalidRegister` (unset value) or `TypeMismatch`.
/// Bounded: data up to `72 + FIELDS` bytes; every offset (`usize`), selector and value.
#[kani::proof]
#[kani::unwind(97)]
fn write_field_writes_exactly_its_field() {
    let data: [u8; HEADER + FIELDS] = kani::any();
    let data_len: usize = kani::any_where(|len: &usize| *len <= HEADER + FIELDS);
    let mut fields = Fields::plain(kani::any(), ballista::ID.to_bytes());
    fields.writable = true;
    let mut entry = AccountMemory::new(fields, data, data_len);
    let offset: usize = kani::any();
    let selector: u8 = kani::any();
    let bytes: [u8; 2] = kani::any();
    let value = any_runtime_value(&bytes);

    let result = write_field(&entry.view(), offset, selector, &value);

    // The field's little-endian bytes, computed by shifts rather than `to_le_bytes`.
    let mut encoded = [0u8; 32];
    let width = match (selector, value) {
        (OP_READ_U64, RuntimeValue::U64(v)) => {
            for j in 0..8 {
                encoded[j] = (v >> (8 * j)) as u8;
            }
            8
        }
        (OP_READ_I64, RuntimeValue::I64(v)) => {
            for j in 0..8 {
                encoded[j] = ((v as u64) >> (8 * j)) as u8;
            }
            8
        }
        (OP_READ_U128, RuntimeValue::U128(v)) => {
            encoded[..16].copy_from_slice(&v);
            16
        }
        (OP_READ_PUBKEY, RuntimeValue::Pubkey(v)) => {
            encoded = v;
            32
        }
        (OP_READ_BOOL, RuntimeValue::Bool(v)) => {
            encoded[0] = v as u8;
            1
        }
        (_, RuntimeValue::Unset) => {
            assert_eq!(result, Err(err(BallistaError::InvalidRegister)));
            0
        }
        _ => {
            assert_eq!(result, Err(err(BallistaError::TypeMismatch)));
            0
        }
    };
    let in_range = width != 0 && offset <= data_len && data_len - offset >= width;
    if width != 0 {
        if in_range {
            assert_eq!(result, Ok(()));
            kani::cover!(width == 32 && offset == HEADER, "a pubkey field written right after the header");
            kani::cover!(offset + width == data_len, "a field ending at the end of the data");
        } else {
            assert_eq!(result, Err(err(BallistaError::InvalidTemplateProgram)));
            kani::cover!(offset > usize::MAX - 8, "an offset whose end overflows");
        }
    }
    for i in 0..HEADER + FIELDS {
        let inside = in_range && i >= offset && i - offset < width;
        let expected = if inside { encoded[i - offset] } else { data[i] };
        assert_eq!(entry.data[i], expected);
    }
}

/// A registry entry is never taken for a template account. Every template-account reader the
/// program uses (`TemplateAccount::parse`, the run path's `finalized_program_unchecked`, and
/// `split_template_account_mut`, behind chunk writes, finalize and cancel) refuses data that starts
/// with an entry's header, for any id. So `CancelTemplate`, which hands an account's lamports to
/// the creator its header names, can never be pointed at an entry Ballista owns, and a run never
/// reads an entry as a program. (The other direction is `open_accepts_exactly_the_documented_header_and_size`:
/// an open accepts only data starting with the entry header.) Bounded: up to 24 bytes after the
/// 72-byte header; the id symbolic.
#[kani::proof]
#[kani::unwind(97)]
fn entries_are_never_read_as_templates() {
    let template = Address::new_from_array(kani::any());
    let id = EntryId { template: &template, index: kani::any(), key: kani::any() };
    let mut bytes: [u8; HEADER + 24] = kani::any();
    bytes[..HEADER].copy_from_slice(&id.header());
    let len: usize = kani::any_where(|len: &usize| (HEADER..=HEADER + 24).contains(len));
    assert!(TemplateAccount::parse(&bytes[..len]).is_err());
    assert!(TemplateAccount::finalized_program_unchecked(&bytes[..len]).is_none());
    assert!(split_template_account_mut(&mut bytes[..len]).is_err());
    kani::cover!(len == HEADER + 8, "an entry the size of a template with an 8-byte payload");
}
