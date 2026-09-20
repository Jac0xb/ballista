//! CVLR helpers for pinocchio programs.
//!
//! `cvlr-solana` builds nondeterministic `solana_program::AccountInfo` values. Pinocchio programs
//! see accounts through `AccountView`, a pointer to the runtime's own input layout: a
//! `RuntimeAccount` header immediately followed by the account data. This crate lays out that
//! memory with havoced contents and hands out views over it, so rules can call pinocchio handlers
//! with fully symbolic accounts.
//!
//! Everything here is independent of any particular program and could live upstream next to
//! `cvlr-solana`.

use cvlr::asserts::cvlr_assume;
use cvlr::nondet::{havoc::alloc_mut_ref_havoced, nondet};
use pinocchio::account::{AccountView, RuntimeAccount, NOT_BORROWED};
use pinocchio::Address;

mod rt_decls {
    #[allow(improper_ctypes)]
    extern "C" {
        /// The prover treats the result as one opaque 32-byte identifier rather than 32 havoced
        /// bytes, which keeps address comparisons cheap. Same symbol `cvlr-solana` uses.
        pub fn CVT_nondet_pubkey() -> [u8; 32];
    }
}

#[cfg(feature = "rt")]
mod rt_impls {
    #[allow(improper_ctypes_definitions)]
    #[no_mangle]
    pub extern "C" fn CVT_nondet_pubkey() -> [u8; 32] {
        [0; 32]
    }

    /// `cvlr-nondet` declares this symbol for havoced allocations but its host runtime does not
    /// export it, so provide the zero-filling implementation here.
    #[no_mangle]
    pub extern "C" fn memhavoc_c(data: *mut u8, size: usize) {
        unsafe { data.write_bytes(0, size) }
    }
}

/// A nondeterministic address the prover treats as an opaque identifier.
pub fn nondet_address() -> Address {
    Address::new_from_array(unsafe { rt_decls::CVT_nondet_pubkey() })
}

/// `N` havoced bytes with a stable `'static` address.
pub fn nondet_bytes<const N: usize>() -> &'static mut [u8; N] {
    alloc_mut_ref_havoced::<[u8; N]>()
}

/// A havoced byte slice whose length is nondeterministic and at most `N`.
pub fn nondet_slice<const N: usize>() -> &'static mut [u8] {
    let bytes = nondet_bytes::<N>();
    let len: usize = nondet();
    cvlr_assume!(len <= N);
    &mut bytes[..len]
}

/// One account exactly as the Solana runtime lays it out for pinocchio: the header, then `DATA`
/// bytes of account data. `data_len` in the header never exceeds `DATA`.
#[repr(C)]
pub struct AccountSlot<const DATA: usize> {
    pub header: RuntimeAccount,
    pub data: [u8; DATA],
}

impl<const DATA: usize> AccountSlot<DATA> {
    /// A havoced slot with an internally consistent header: no outstanding borrows, boolean
    /// flags that are `0` or `1`, opaque nondeterministic address and owner, nondeterministic
    /// lamports, and a data length that fits the slot. The data bytes stay fully symbolic.
    pub fn nondet() -> &'static mut Self {
        let slot = alloc_mut_ref_havoced::<Self>();
        slot.header.borrow_state = NOT_BORROWED;
        slot.header.is_signer = u8::from(nondet::<bool>());
        slot.header.is_writable = u8::from(nondet::<bool>());
        slot.header.executable = u8::from(nondet::<bool>());
        slot.header.padding = [0; 4];
        slot.header.address = nondet_address();
        slot.header.owner = nondet_address();
        slot.header.lamports = nondet();
        let data_len: u64 = nondet();
        cvlr_assume!(data_len <= DATA as u64);
        slot.header.data_len = data_len;
        slot
    }

    /// A view over this slot, as the program would receive it from the entrypoint.
    pub fn view(&mut self) -> AccountView {
        // SAFETY: `#[repr(C)]` places `data` immediately after `header`, and `data_len` is kept
        // within `DATA`, which is the invariant `AccountView` requires.
        unsafe { AccountView::new_unchecked(self as *mut Self as *mut RuntimeAccount) }
    }

    /// The live portion of the account data.
    pub fn data_mut(&mut self) -> &mut [u8] {
        let len = self.header.data_len as usize;
        &mut self.data[..len]
    }
}

/// A nondeterministic account view with up to `DATA` bytes of data.
pub fn nondet_account_view<const DATA: usize>() -> AccountView {
    AccountSlot::<DATA>::nondet().view()
}

/// `N` independent nondeterministic account views.
pub fn nondet_account_views<const N: usize, const DATA: usize>() -> [AccountView; N] {
    core::array::from_fn(|_| nondet_account_view::<DATA>())
}

#[cfg(all(test, feature = "rt"))]
mod tests {
    use super::*;

    #[test]
    fn slots_present_a_consistent_account_view() {
        let slot = AccountSlot::<64>::nondet();
        slot.header.is_signer = 1;
        slot.header.lamports = 42;
        slot.header.data_len = 16;
        slot.data[0] = 7;
        let view = slot.view();
        assert!(view.is_signer());
        assert_eq!(view.lamports(), 42);
        assert_eq!(view.data_len(), 16);
        assert_eq!(view.try_borrow().unwrap()[0], 7);
        assert_eq!(view.address(), &slot.header.address);
        assert_eq!(view.owner(), &slot.header.owner);
        assert_eq!(
            core::mem::size_of::<AccountSlot<64>>(),
            core::mem::size_of::<RuntimeAccount>() + 64
        );
    }
}
