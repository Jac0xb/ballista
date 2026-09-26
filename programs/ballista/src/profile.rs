//! Compute-unit sampling, compiled only under the `cu-profile` feature.
//!
//! The release binary has none of this: every call below is `#[cfg]`-gated away, so instrumenting
//! a phase costs nothing in production. Under the feature each `mark` reads
//! `sol_remaining_compute_units` and stores it; the run ends by emitting one `sol_log_data` record
//! that `cargo test --features cu-profile -- phase_profile` decodes into the table in
//! `docs/cu-profile.md`. The record goes out as the instruction's return data, which is where a
//! Mollusk harness can read it without parsing logs.
//!
//! Reading the counter is itself a syscall, so the first two marks are taken back to back and the
//! decoder subtracts that calibration from every interval.

// The tags and sizes below describe the record even in builds that never emit one.
#![cfg_attr(not(feature = "cu-profile"), allow(dead_code))]

/// Magic of the profile record, so a decoder can tell it from the run event.
pub const PROFILE_MAGIC: [u8; 4] = *b"BCU1";

/// Phase tags, in the order a run passes through them.
pub const TAG_CALIBRATE_A: u8 = 0;
pub const TAG_CALIBRATE_B: u8 = 1;
pub const TAG_DISPATCHED: u8 = 2;
pub const TAG_TEMPLATE_LOADED: u8 = 3;
pub const TAG_ACCOUNTS_VALIDATED: u8 = 4;
pub const TAG_INPUTS_PARSED: u8 = 5;
pub const TAG_REGISTERS_ALLOCATED: u8 = 6;
pub const TAG_STATE_ALLOCATED: u8 = 8;
pub const TAG_EXECUTED: u8 = 7;

pub const MAX_MARKS: usize = 16;

#[cfg(feature = "cu-profile")]
mod enabled {
    use super::*;

    /// Samples live in the last 512 bytes of the default 32 KiB heap. An SBF program cannot have
    /// writable statics, and the VM hands every instruction a zeroed heap, so a fixed slot near
    /// the top is both legal and already initialized. Nothing Ballista allocates comes close to
    /// this address.
    const SLOT: *mut u8 = (0x3_0000_0000u64 + 32 * 1024 - 512) as *mut u8;
    const OFF_LEN: usize = 0;
    const OFF_MARKS: usize = 8;
    const OFF_CPI_UNITS: usize = OFF_MARKS + MAX_MARKS * 16;
    const OFF_CPI_OPEN: usize = OFF_CPI_UNITS + 8;
    const OFF_CPI_COUNT: usize = OFF_CPI_OPEN + 8;
    const OFF_SETUP_UNITS: usize = OFF_CPI_COUNT + 8;
    const OFF_SETUP_OPEN: usize = OFF_SETUP_UNITS + 8;

    #[inline(always)]
    unsafe fn load(offset: usize) -> u64 {
        SLOT.add(offset).cast::<u64>().read()
    }

    #[inline(always)]
    unsafe fn store(offset: usize, value: u64) {
        SLOT.add(offset).cast::<u64>().write(value);
    }

    /// Compute units left in the budget, or zero off chain.
    #[inline(always)]
    pub fn remaining() -> u64 {
        #[cfg(target_os = "solana")]
        unsafe {
            pinocchio::syscalls::sol_remaining_compute_units()
        }
        #[cfg(not(target_os = "solana"))]
        0
    }

    /// Records the budget left at one phase boundary.
    #[inline(always)]
    pub fn mark(tag: u8) {
        let value = remaining();
        #[cfg(target_os = "solana")]
        unsafe {
            let len = load(OFF_LEN) as usize;
            if len < MAX_MARKS {
                store(OFF_MARKS + len * 16, tag as u64);
                store(OFF_MARKS + len * 16 + 8, value);
                store(OFF_LEN, len as u64 + 1);
            }
        }
        #[cfg(not(target_os = "solana"))]
        let _ = (tag, value);
    }

    /// Opens the window that builds one invocation: resolving accounts and encoding its data.
    #[inline(always)]
    pub fn setup_begin() {
        let value = remaining();
        #[cfg(target_os = "solana")]
        unsafe {
            store(OFF_SETUP_OPEN, value)
        };
        #[cfg(not(target_os = "solana"))]
        let _ = value;
    }

    /// Closes it, accumulating what the build cost.
    #[inline(always)]
    pub fn setup_end() {
        let value = remaining();
        #[cfg(target_os = "solana")]
        unsafe {
            let spent = load(OFF_SETUP_OPEN).saturating_sub(value);
            store(OFF_SETUP_UNITS, load(OFF_SETUP_UNITS) + spent);
        }
        #[cfg(not(target_os = "solana"))]
        let _ = value;
    }

    /// Opens the window charged to the invoke syscall and the callee.
    #[inline(always)]
    pub fn cpi_begin() {
        let value = remaining();
        #[cfg(target_os = "solana")]
        unsafe {
            store(OFF_CPI_OPEN, value)
        };
        #[cfg(not(target_os = "solana"))]
        let _ = value;
    }

    /// Closes it, accumulating the units the invoke cost.
    #[inline(always)]
    pub fn cpi_end() {
        let value = remaining();
        #[cfg(target_os = "solana")]
        unsafe {
            let spent = load(OFF_CPI_OPEN).saturating_sub(value);
            store(OFF_CPI_UNITS, load(OFF_CPI_UNITS) + spent);
            store(OFF_CPI_COUNT, load(OFF_CPI_COUNT) + 1);
        }
        #[cfg(not(target_os = "solana"))]
        let _ = value;
    }

    /// Emits the record: magic, mark count, `(tag, remaining)` pairs, then the CPI totals.
    pub fn report() {
        let mut record = [0u8; 4 + 1 + MAX_MARKS * 9 + 20];
        record[..4].copy_from_slice(&PROFILE_MAGIC);
        #[cfg(target_os = "solana")]
        let (len, cpi_units, cpi_count, setup_units) = unsafe {
            (
                load(OFF_LEN) as usize,
                load(OFF_CPI_UNITS),
                load(OFF_CPI_COUNT) as u32,
                load(OFF_SETUP_UNITS),
            )
        };
        #[cfg(not(target_os = "solana"))]
        let (len, cpi_units, cpi_count, setup_units) = (0usize, 0u64, 0u32, 0u64);
        record[4] = len as u8;
        let mut at = 5;
        for index in 0..len {
            #[cfg(target_os = "solana")]
            let (tag, value) = unsafe {
                (
                    load(OFF_MARKS + index * 16) as u8,
                    load(OFF_MARKS + index * 16 + 8),
                )
            };
            #[cfg(not(target_os = "solana"))]
            let (tag, value) = (0u8, 0u64);
            record[at] = tag;
            record[at + 1..at + 9].copy_from_slice(&value.to_le_bytes());
            at += 9;
        }
        record[at..at + 8].copy_from_slice(&cpi_units.to_le_bytes());
        record[at + 8..at + 12].copy_from_slice(&cpi_count.to_le_bytes());
        record[at + 12..at + 20].copy_from_slice(&setup_units.to_le_bytes());
        at += 20;
        emit(&record[..at]);
    }

    #[inline(always)]
    fn emit(bytes: &[u8]) {
        #[cfg(target_os = "solana")]
        unsafe {
            pinocchio::syscalls::sol_set_return_data(bytes.as_ptr(), bytes.len() as u64);
        }
        #[cfg(not(target_os = "solana"))]
        let _ = bytes;
    }
}

#[cfg(not(feature = "cu-profile"))]
mod enabled {
    #[inline(always)]
    pub fn mark(_tag: u8) {}
    #[inline(always)]
    pub fn cpi_begin() {}
    #[inline(always)]
    pub fn cpi_end() {}
    #[inline(always)]
    pub fn setup_begin() {}
    #[inline(always)]
    pub fn setup_end() {}
    #[inline(always)]
    pub fn report() {}
}

pub(crate) use enabled::{cpi_begin, cpi_end, mark, report, setup_begin, setup_end};
