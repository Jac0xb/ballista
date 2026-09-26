//! Program-derived addresses, computed from the syscalls the runtime's own PDA syscalls are made
//! of.
//!
//! `sol_try_find_program_address` and `sol_create_program_address` charge 1,500 compute units for
//! every bump they try. One attempt is a SHA-256 of `seeds || bump || program id ||
//! "ProgramDerivedAddress"` and a check that the digest is not an ed25519 point, and both steps are
//! syscalls of their own: `sol_sha256` charges 85 units plus one per two bytes of each slice (at
//! least 10 per slice), and `sol_curve_validate_point` charges 159 for an Edwards point. Calling
//! them directly makes an attempt about a fifth as expensive and yields exactly the same address:
//! the runtime's PDA syscalls hash the same bytes and reject the digest on the same test, a
//! `CompressedEdwardsY` decompression.
//!
//! The preimage is one contiguous buffer hashed as a single slice. A slice costs at least 10 units
//! however short it is, so hashing the seeds, bump, program id and marker separately would cost
//! more than their bytes do. Only the bump byte changes between attempts.
use core::mem::MaybeUninit;

use solana_address::Address;

pub const TEMPLATE_SEED: &[u8] = b"template";

/// The marker every PDA preimage ends with.
pub const PDA_MARKER: &[u8; 21] = b"ProgramDerivedAddress";
/// Longest seed a derivation accepts, as in `create_program_address`.
pub const MAX_SEED_LEN: usize = 32;
/// Most seeds a derivation accepts, the bump included, as in `create_program_address`.
pub const MAX_SEEDS: usize = 16;
/// The bytes after the seeds in a canonical search: the bump, the program id, and the marker.
pub const PREIMAGE_SUFFIX_LEN: usize = 1 + 32 + PDA_MARKER.len();
/// Room for any seed list `create_program_address` accepts.
const GENERAL_PREIMAGE_LEN: usize = preimage_capacity(MAX_SEEDS * MAX_SEED_LEN);

/// Buffer length for a preimage whose seeds total at most `seed_bytes` bytes.
pub const fn preimage_capacity(seed_bytes: usize) -> usize {
    seed_bytes + PREIMAGE_SUFFIX_LEN
}

/// The bytes a derivation hashes, built in place: the seeds, then (for a search) the bump, then
/// the program id and marker. The buffer is never zeroed; every byte hashed is written first.
pub struct Preimage<const N: usize> {
    bytes: MaybeUninit<[u8; N]>,
    /// Seed bytes written so far. `push_seed` keeps `seeds_len + PREIMAGE_SUFFIX_LEN <= N`.
    seeds_len: usize,
    seed_count: usize,
}

impl<const N: usize> Preimage<N> {
    const FITS_SUFFIX: () = assert!(N >= PREIMAGE_SUFFIX_LEN);

    #[inline(always)]
    pub fn new() -> Self {
        let () = Self::FITS_SUFFIX;
        Self {
            bytes: MaybeUninit::uninit(),
            seeds_len: 0,
            seed_count: 0,
        }
    }

    #[inline(always)]
    fn base(&mut self) -> *mut u8 {
        self.bytes.as_mut_ptr().cast()
    }

    /// Appends one seed, or returns `None` when `create_program_address` would refuse it: longer
    /// than `MAX_SEED_LEN`, or past `MAX_SEEDS`. Also `None` if the buffer is too small.
    ///
    /// Offsets here and in `seal` are bounded by `N`, so they add without overflow checks, which
    /// the release profile would otherwise emit for each sum.
    #[inline(always)]
    pub fn push_seed(&mut self, seed: &[u8]) -> Option<()> {
        let len = seed.len();
        if len > MAX_SEED_LEN
            || self.seed_count >= MAX_SEEDS
            || self.seeds_len.wrapping_add(len) > N - PREIMAGE_SUFFIX_LEN
        {
            return None;
        }
        // SAFETY: the checks above keep the write inside the buffer, and a borrowed seed cannot
        // overlap a buffer this value owns. Addresses (32 bytes) and bumps (1 byte) are copied at a
        // fixed size, which avoids a `sol_memcpy_` call; any other length goes through one.
        unsafe {
            let at = self.base().add(self.seeds_len);
            if len == MAX_SEED_LEN {
                at.cast::<[u8; MAX_SEED_LEN]>()
                    .write_unaligned(seed.as_ptr().cast::<[u8; MAX_SEED_LEN]>().read_unaligned());
            } else if len == 1 {
                at.write(*seed.as_ptr());
            } else {
                core::ptr::copy_nonoverlapping(seed.as_ptr(), at, len);
            }
        }
        self.seeds_len = self.seeds_len.wrapping_add(len);
        self.seed_count = self.seed_count.wrapping_add(1);
        Some(())
    }

    /// `Address::create_program_address(seeds, program_id)` for the seeds pushed so far, the bump
    /// among them: the address, or `None` when the digest lies on the curve.
    #[inline(always)]
    pub fn create(&mut self, program_id: &Address) -> Option<Address> {
        let len = self.seal(self.seeds_len, program_id);
        let input = HashInput::new(self.base(), len);
        let mut digest = [0u8; 32];
        // SAFETY: the seeds, program id and marker fill the first `len` bytes.
        unsafe { input.off_curve(&mut digest) }.then(|| Address::new_from_array(digest))
    }

    /// `Address::try_find_program_address(seeds, program_id)` for the seeds pushed so far: the
    /// first bump from 255 down whose address is off the curve. Like the runtime it stops at 1 and
    /// never tries 0, and finds nothing when the bump would be a seventeenth seed.
    #[inline(always)]
    pub fn find(&mut self, program_id: &Address) -> Option<(Address, u8)> {
        if self.seed_count >= MAX_SEEDS {
            return None;
        }
        let bump_at = self.seeds_len;
        let len = self.seal(bump_at.wrapping_add(1), program_id);
        let base = self.base();
        let input = HashInput::new(base, len);
        let mut digest = [0u8; 32];
        let mut bump = u8::MAX as u64;
        loop {
            // SAFETY: `bump_at < len <= N`. Once the bump is written the seeds, bump, program id
            // and marker fill the first `len` bytes.
            let off_curve = unsafe {
                base.add(bump_at).write(bump as u8);
                input.off_curve(&mut digest)
            };
            if off_curve {
                return Some((Address::new_from_array(digest), bump as u8));
            }
            bump -= 1;
            if bump == 0 {
                return None;
            }
        }
    }

    /// Writes the program id and marker at `at`, which is at most `seeds_len + 1`, and returns
    /// where they end: the preimage's length.
    #[inline(always)]
    fn seal(&mut self, at: usize, program_id: &Address) -> usize {
        // SAFETY: `at + 53 <= seeds_len + PREIMAGE_SUFFIX_LEN <= N`.
        unsafe {
            let id = self.base().add(at);
            id.cast::<[u8; 32]>()
                .write_unaligned(*program_id.as_array());
            id.add(32)
                .cast::<[u8; PDA_MARKER.len()]>()
                .write_unaligned(*PDA_MARKER);
        }
        at.wrapping_add(32 + PDA_MARKER.len())
    }
}

impl<const N: usize> Default for Preimage<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// A preimage as `sol_sha256` reads its input: a list of `(pointer, length)` slices, here one.
/// Built once per derivation, so a search only rewrites the bump byte between attempts.
#[repr(C)]
struct HashInput {
    ptr: *const u8,
    len: u64,
}

impl HashInput {
    #[inline(always)]
    fn new(ptr: *const u8, len: usize) -> Self {
        Self {
            ptr,
            len: len as u64,
        }
    }

    /// One attempt: hashes the preimage into `digest` and reports whether the digest is off the
    /// curve, which is what makes it a program address.
    ///
    /// # Safety
    ///
    /// The `len` bytes at `ptr` must be initialized.
    #[inline(always)]
    unsafe fn off_curve(&self, digest: &mut [u8; 32]) -> bool {
        #[cfg(target_os = "solana")]
        {
            /// `solana-curve25519`'s id for Edwards points.
            const CURVE25519_EDWARDS: u64 = 0;
            pinocchio::syscalls::sol_sha256((self as *const Self).cast(), 1, digest.as_mut_ptr());
            // Zero means the bytes decompress to a curve point.
            pinocchio::syscalls::sol_curve_validate_point(
                CURVE25519_EDWARDS,
                digest.as_ptr(),
                core::ptr::null_mut(),
            ) != 0
        }
        #[cfg(not(target_os = "solana"))]
        {
            let preimage = core::slice::from_raw_parts(self.ptr, self.len as usize);
            *digest = solana_sha256_hasher::hashv(&[preimage]).to_bytes();
            !solana_address::bytes_are_curve_point(digest)
        }
    }
}

/// `Address::try_find_program_address`, at about 300 compute units per bump tried instead of 1,500.
///
/// Returns `None` wherever that function does, and also for more than sixteen seeds or a seed over
/// 32 bytes, where the syscall aborts the transaction instead.
#[inline(never)]
pub fn try_find_program_address(seeds: &[&[u8]], program_id: &Address) -> Option<(Address, u8)> {
    let mut preimage = Preimage::<GENERAL_PREIMAGE_LEN>::new();
    for seed in seeds {
        preimage.push_seed(seed)?;
    }
    preimage.find(program_id)
}

/// `Address::create_program_address`, at about 300 compute units instead of 1,500: the address,
/// or `None` for an on-curve digest, more than sixteen seeds, or a seed over 32 bytes.
#[inline(never)]
pub fn create_program_address(seeds: &[&[u8]], program_id: &Address) -> Option<Address> {
    let mut preimage = Preimage::<GENERAL_PREIMAGE_LEN>::new();
    for seed in seeds {
        preimage.push_seed(seed)?;
    }
    preimage.create(program_id)
}

/// Seed bytes of a template PDA: the tag, the creator, and the little-endian id.
const TEMPLATE_PREIMAGE_LEN: usize = preimage_capacity(TEMPLATE_SEED.len() + 32 + 2);

/// The template PDA and its canonical bump, exactly what `Address::find_program_address` returns
/// for `["template", creator, id]`. The seeds have fixed lengths, so every copy has a fixed size.
#[inline(never)]
pub fn get_template_address(creator: &Address, id: u16) -> (Address, u8) {
    let mut preimage = Preimage::<TEMPLATE_PREIMAGE_LEN>::new();
    preimage
        .push_seed(TEMPLATE_SEED)
        .and_then(|()| preimage.push_seed(creator.as_ref()))
        .and_then(|()| preimage.push_seed(&id.to_le_bytes()))
        .and_then(|()| preimage.find(&crate::ID))
        .unwrap_or_else(|| panic!("Unable to find a viable program address bump seed"))
}

#[cfg(test)]
mod tests {
    use super::*;

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

        fn bytes(&mut self, len: usize) -> Vec<u8> {
            (0..len).map(|_| self.next() as u8).collect()
        }

        fn address(&mut self) -> Address {
            Address::new_from_array(self.bytes(32).try_into().unwrap())
        }
    }

    #[test]
    fn the_limits_and_marker_are_the_runtimes() {
        assert_eq!(PDA_MARKER, solana_address::PDA_MARKER);
        assert_eq!(MAX_SEED_LEN, solana_address::MAX_SEED_LEN);
        assert_eq!(MAX_SEEDS, solana_address::MAX_SEEDS);
    }

    /// The host build hashes with `sha2` and tests the curve with `curve25519-dalek`, as
    /// `solana-address` does, so this pins the preimage layout, the bump order and the seed
    /// bounds; the syscall path is compared against the runtime in the Mollusk suite.
    #[test]
    fn find_and_create_match_the_address_crate() {
        let mut rng = SplitMix(0xba11_157a);
        let mut deepest = u8::MAX;
        for case in 0..1_000 {
            let count = 1 + (rng.next() % 15) as usize;
            let seeds: Vec<Vec<u8>> = (0..count)
                .map(|_| {
                    let len = (rng.next() % (MAX_SEED_LEN as u64 + 1)) as usize;
                    rng.bytes(len)
                })
                .collect();
            let program_id = rng.address();
            let slices: Vec<&[u8]> = seeds.iter().map(Vec::as_slice).collect();

            let expected = Address::try_find_program_address(&slices, &program_id);
            assert_eq!(
                try_find_program_address(&slices, &program_id),
                expected,
                "case {case}"
            );
            let (_, canonical) = expected.expect("a bump exists");
            deepest = deepest.min(canonical);

            // Every bump from the canonical one up, all but which land on the curve, plus a
            // random one that may land on either side, and zero, which only a caller can supply.
            let random = rng.next() as u8;
            for bump in (canonical..=u8::MAX).chain([random, 0]) {
                let bump_seed = [bump];
                let mut with_bump = slices.clone();
                with_bump.push(&bump_seed);
                assert_eq!(
                    create_program_address(&with_bump, &program_id),
                    Address::create_program_address(&with_bump, &program_id).ok(),
                    "case {case}, bump {bump}"
                );
            }
        }
        assert!(
            deepest <= 250,
            "the cases reach searches six bumps deep: {deepest}"
        );
    }

    #[test]
    fn template_addresses_match_find_program_address() {
        let mut rng = SplitMix(7);
        for _ in 0..500 {
            let creator = rng.address();
            let id = rng.next() as u16;
            assert_eq!(
                get_template_address(&creator, id),
                Address::find_program_address(
                    &[TEMPLATE_SEED, creator.as_ref(), &id.to_le_bytes()],
                    &crate::ID
                )
            );
        }
    }

    #[test]
    fn seed_bounds_match_the_address_crate() {
        let program_id = Address::new_from_array([3; 32]);
        let full = [9u8; MAX_SEED_LEN];
        let long = [1u8; MAX_SEED_LEN + 1];
        for count in [0, 1, 14, 15, 16, 17] {
            let seeds: Vec<&[u8]> = vec![&full; count];
            assert_eq!(
                try_find_program_address(&seeds, &program_id),
                Address::try_find_program_address(&seeds, &program_id),
                "{count} full seeds"
            );
            assert_eq!(
                create_program_address(&seeds, &program_id),
                Address::create_program_address(&seeds, &program_id).ok(),
                "{count} full seeds"
            );
        }
        let with_long: [&[u8]; 2] = [&full, &long];
        assert_eq!(try_find_program_address(&with_long, &program_id), None);
        assert_eq!(create_program_address(&with_long, &program_id), None);
        assert!(Address::create_program_address(&with_long, &program_id).is_err());
    }
}
