//! Program-derived addresses (`programs/ballista/src/utils/pda.rs`). The derivation builds its
//! preimage in an uninitialized buffer through raw pointer writes, on a hand-argued bound, then
//! hashes it and checks the digest against the curve, once per bump.
//!
//! SHA-256 and the curve check are stubbed. The hash stub asserts that every attempt hashes exactly
//! the documented preimage, `seeds ‖ bump ‖ program id ‖ "ProgramDerivedAddress"` (no bump for
//! `create_program_address`), byte for byte: an uninitialized byte reads as an arbitrary value in
//! Kani, so a byte the code forgot to write fails that check too. It returns a digest that names the
//! attempt. The curve stub answers from a symbolic oracle. Kani's pointer checks cover every write.
//! What stays unproved is the hash and the curve check themselves.

use ballista::utils::pda::{
    create_program_address, get_registry_address, get_template_address, try_find_program_address,
    PDA_MARKER, TEMPLATE_SEED,
};
use ballista_common::template::REGISTRY_SEED;
use pinocchio::Address;

/// Attempts the oracle answers symbolically; every later attempt is off the curve, which bounds the
/// search loop.
const ATTEMPTS: usize = 3;

/// Longest preimage whose bytes the stub checks.
const CHECKED: usize = 160;

struct Oracle {
    /// The preimage every attempt must hash, with the bump byte (if any) at `bump_at`.
    expected: [u8; CHECKED],
    len: usize,
    bump_at: Option<usize>,
    check: bool,
    /// Attempts so far.
    calls: usize,
    on_curve: [bool; ATTEMPTS],
    /// The answer for attempts past `ATTEMPTS`.
    later_on_curve: bool,
}

static mut ORACLE: Oracle = Oracle {
    expected: [0; CHECKED],
    len: 0,
    bump_at: None,
    check: false,
    calls: 0,
    on_curve: [false; ATTEMPTS],
    later_on_curve: false,
};

fn oracle() -> &'static mut Oracle {
    // SAFETY: harnesses are single-threaded, and no reference outlives the statement using it.
    unsafe { &mut *core::ptr::addr_of_mut!(ORACLE) }
}

/// Stub for `solana_sha256_hasher::hashv`.
#[allow(dead_code)] // referenced only from `#[kani::stub]`
pub fn hashv(vals: &[&[u8]]) -> solana_hash::Hash {
    let oracle = oracle();
    assert_eq!(vals.len(), 1, "the preimage is hashed as one slice");
    let preimage = vals[0];
    if oracle.check {
        assert_eq!(preimage.len(), oracle.len, "preimage length");
        for i in 0..preimage.len() {
            let expected = match oracle.bump_at {
                Some(at) if i == at => (255 - oracle.calls) as u8,
                _ => oracle.expected[i],
            };
            assert_eq!(preimage[i], expected, "preimage byte");
        }
    }
    oracle.calls += 1;
    let mut digest = [0u8; 32];
    digest[0] = oracle.calls as u8;
    solana_hash::Hash::new_from_array(digest)
}

/// Stub for `solana_address::bytes_are_curve_point`: the oracle's answer for the attempt the
/// digest names.
#[allow(dead_code)] // referenced only from `#[kani::stub]`
pub fn bytes_are_curve_point<T: AsRef<[u8]>>(bytes: T) -> bool {
    let oracle = oracle();
    let attempt = bytes.as_ref()[0] as usize - 1;
    if attempt < ATTEMPTS {
        oracle.on_curve[attempt]
    } else {
        oracle.later_on_curve
    }
}

/// Arms the oracle to expect `parts` concatenated, with a bump byte after the first `bump_after`
/// bytes when the derivation searches.
fn expect(parts: &[&[u8]], bump: bool) {
    let oracle = oracle();
    let mut at = 0;
    let mut bump_at = None;
    for (index, part) in parts.iter().enumerate() {
        if bump && index == parts.len() - 2 {
            bump_at = Some(at);
            at += 1;
        }
        for byte in part.iter() {
            oracle.expected[at] = *byte;
            at += 1;
        }
    }
    oracle.len = at;
    oracle.bump_at = bump_at;
    oracle.check = true;
    oracle.calls = 0;
    oracle.on_curve = kani::any();
    oracle.later_on_curve = false;
}

/// The first attempt the oracle puts off the curve.
fn first_off_curve() -> usize {
    let oracle = oracle();
    (0..ATTEMPTS).find(|attempt| !oracle.on_curve[*attempt]).unwrap_or(ATTEMPTS)
}

/// Up to 3 seeds of up to 33 bytes each, contents symbolic.
fn any_seeds<'a>(storage: &'a [[u8; 33]; 3], seeds: &'a mut [&'a [u8]; 3]) -> &'a [&'a [u8]] {
    let count: usize = kani::any_where(|n: &usize| *n <= 3);
    for i in 0..3 {
        let len: usize = kani::any_where(|n: &usize| *n <= 33);
        seeds[i] = &storage[i][..len];
    }
    &seeds[..count]
}

/// `try_find_program_address` refuses a seed over 32 bytes without hashing anything; otherwise it
/// hashes exactly `seeds ‖ bump ‖ program id ‖ marker` for bump 255, 254, … and returns the
/// digest and bump of the first attempt off the curve. Bounded: up to 3 seeds of up to 33 bytes
/// (so every copy path: 1 byte, 32 bytes, other lengths, and a refusal), any program id; the first
/// 3 attempts' curve answers are symbolic and later ones off the curve.
#[kani::proof]
#[kani::unwind(161)]
#[kani::stub(solana_sha256_hasher::hashv, crate::pda::hashv)]
#[kani::stub(solana_address::bytes_are_curve_point, crate::pda::bytes_are_curve_point)]
fn search_hashes_the_documented_preimage() {
    let storage: [[u8; 33]; 3] = kani::any();
    let mut slots: [&[u8]; 3] = [&[]; 3];
    let seeds = any_seeds(&storage, &mut slots);
    let program_id: [u8; 32] = kani::any();
    let mut parts: [&[u8]; 6] = [&[]; 6];
    parts[..seeds.len()].copy_from_slice(seeds);
    parts[seeds.len()] = &program_id;
    parts[seeds.len() + 1] = PDA_MARKER;
    expect(&parts[..seeds.len() + 2], true);

    let result = try_find_program_address(seeds, &Address::new_from_array(program_id));

    if seeds.iter().any(|seed| seed.len() > 32) {
        assert!(result.is_none());
        assert_eq!(oracle().calls, 0);
        kani::cover!(seeds.len() == 3, "a 33-byte seed among three is refused");
    } else {
        let attempt = first_off_curve();
        assert_eq!(oracle().calls, attempt + 1);
        let (address, bump) = result.expect("an off-curve attempt was found");
        assert_eq!(bump as usize, 255 - attempt);
        assert_eq!(address.to_bytes()[0] as usize, attempt + 1, "the digest of that attempt");
        kani::cover!(attempt == ATTEMPTS && seeds.len() == 3, "the fourth bump, after three seeds");
        kani::cover!(
            seeds.len() == 3 && seeds[0].len() == 32 && seeds[1].len() == 1 && seeds[2].len() == 7,
            "every copy path: 32 bytes, 1 byte, and a general length"
        );
    }
}

/// `create_program_address` refuses a seed over 32 bytes without hashing; otherwise it hashes
/// exactly `seeds ‖ program id ‖ marker` once (the caller's bump is the last seed) and returns the
/// digest exactly when it is off the curve. Bounded as `search_hashes_the_documented_preimage`.
#[kani::proof]
#[kani::unwind(161)]
#[kani::stub(solana_sha256_hasher::hashv, crate::pda::hashv)]
#[kani::stub(solana_address::bytes_are_curve_point, crate::pda::bytes_are_curve_point)]
fn create_hashes_the_documented_preimage() {
    let storage: [[u8; 33]; 3] = kani::any();
    let mut slots: [&[u8]; 3] = [&[]; 3];
    let seeds = any_seeds(&storage, &mut slots);
    let program_id: [u8; 32] = kani::any();
    let mut parts: [&[u8]; 5] = [&[]; 5];
    parts[..seeds.len()].copy_from_slice(seeds);
    parts[seeds.len()] = &program_id;
    parts[seeds.len() + 1] = PDA_MARKER;
    expect(&parts[..seeds.len() + 2], false);

    let result = create_program_address(seeds, &Address::new_from_array(program_id));

    if seeds.iter().any(|seed| seed.len() > 32) {
        assert!(result.is_none());
        assert_eq!(oracle().calls, 0);
    } else {
        assert_eq!(oracle().calls, 1);
        assert_eq!(result.is_some(), !oracle().on_curve[0]);
        kani::cover!(result.is_some() && seeds.len() == 3, "three seeds give an address");
        kani::cover!(result.is_none(), "an on-curve digest gives none");
    }
}

/// When every digest lies on the curve, the search tries bumps 255 down to 1, each once, never 0,
/// and finds nothing. Bounded: no seeds; the loop runs to its end.
#[kani::proof]
#[kani::unwind(257)]
#[kani::stub(solana_sha256_hasher::hashv, crate::pda::hashv)]
#[kani::stub(solana_address::bytes_are_curve_point, crate::pda::bytes_are_curve_point)]
fn search_tries_every_bump_from_255_to_1() {
    let oracle = oracle();
    oracle.check = false;
    oracle.calls = 0;
    oracle.on_curve = [true; ATTEMPTS];
    oracle.later_on_curve = true;
    assert_eq!(try_find_program_address(&[], &Address::new_from_array([3; 32])), None);
    assert_eq!(oracle.calls, 255);
    kani::cover!(oracle.calls == 255, "the search ran to bump 1");
}

/// The seed limits, at the buffer's capacity: a search takes at most 15 seeds (the bump is the
/// sixteenth) and a creation at most 16, as the runtime's derivations do; past that both return
/// `None` without hashing, and at the limit, with every seed 32 bytes, every write stays inside
/// the buffer. Seed contents symbolic; the curve answer for the first attempt is symbolic.
#[kani::proof]
#[kani::unwind(18)]
#[kani::stub(solana_sha256_hasher::hashv, crate::pda::hashv)]
#[kani::stub(solana_address::bytes_are_curve_point, crate::pda::bytes_are_curve_point)]
fn seed_count_limits_hold_at_full_capacity() {
    let seed: [u8; 32] = kani::any();
    let seeds = [&seed[..]; 17];
    let program_id = Address::new_from_array(kani::any());
    let oracle = oracle();
    oracle.check = false;
    oracle.on_curve = kani::any();
    oracle.later_on_curve = false;

    oracle.calls = 0;
    assert!(try_find_program_address(&seeds[..16], &program_id).is_none());
    assert_eq!(oracle.calls, 0);
    oracle.calls = 0;
    assert!(try_find_program_address(&seeds[..15], &program_id).is_some());
    assert!(oracle.calls >= 1);

    oracle.calls = 0;
    assert!(create_program_address(&seeds[..17], &program_id).is_none());
    assert_eq!(oracle.calls, 0);
    oracle.calls = 0;
    let created = create_program_address(&seeds[..16], &program_id);
    assert_eq!(oracle.calls, 1);
    assert_eq!(created.is_some(), !oracle.on_curve[0]);
    kani::cover!(created.is_some(), "sixteen 32-byte seeds give an address");
}

/// A template's address hashes `"template" ‖ creator ‖ id (2 bytes, little-endian) ‖ bump ‖
/// Ballista's id ‖ marker`, and a registry entry's `"registry" ‖ template ‖ [index] ‖ key ‖ bump ‖
/// Ballista's id ‖ marker`, each in a buffer sized exactly for it. Every input symbolic; curve
/// answers as `search_hashes_the_documented_preimage`.
#[kani::proof]
#[kani::unwind(161)]
#[kani::stub(solana_sha256_hasher::hashv, crate::pda::hashv)]
#[kani::stub(solana_address::bytes_are_curve_point, crate::pda::bytes_are_curve_point)]
fn template_and_registry_addresses_hash_their_documented_seeds() {
    let program_id = ballista::ID.to_bytes();
    if kani::any() {
        let creator: [u8; 32] = kani::any();
        let id: u16 = kani::any();
        let id_le = id.to_le_bytes();
        expect(&[TEMPLATE_SEED, &creator, &id_le, &program_id, PDA_MARKER], true);
        let (address, bump) = get_template_address(&Address::new_from_array(creator), id);
        let attempt = first_off_curve();
        assert_eq!(bump as usize, 255 - attempt);
        assert_eq!(address.to_bytes()[0] as usize, attempt + 1);
        kani::cover!(attempt == 2, "a template address on the third bump");
    } else {
        let template: [u8; 32] = kani::any();
        let index: u8 = kani::any();
        let key: [u8; 32] = kani::any();
        expect(&[REGISTRY_SEED, &template, &[index], &key, &program_id, PDA_MARKER], true);
        let (address, bump) = get_registry_address(&Address::new_from_array(template), index, &key)
            .expect("an off-curve attempt was found");
        let attempt = first_off_curve();
        assert_eq!(bump as usize, 255 - attempt);
        assert_eq!(address.to_bytes()[0] as usize, attempt + 1);
        kani::cover!(attempt == 0, "a registry address on the first bump");
    }
}
