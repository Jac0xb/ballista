//! Account memory laid out as the runtime hands it to the entrypoint, for harnesses that need an
//! `AccountView`. A view is a pointer to a `RuntimeAccount` header followed directly by the data;
//! nothing here goes through a syscall.

use core::mem::size_of;

use pinocchio::account::{RuntimeAccount, NOT_BORROWED};
use pinocchio::{AccountView, Address};

/// One account: the 88-byte runtime header, then a `DATA`-byte data region of which the first
/// `header.data_len` bytes are the account's data.
#[repr(C)]
pub struct AccountMemory<const DATA: usize> {
    pub header: RuntimeAccount,
    pub data: [u8; DATA],
}

const _: () = assert!(size_of::<RuntimeAccount>() == 88);

/// The fields of a runtime account header a harness chooses.
#[derive(Clone, Copy)]
pub struct Fields {
    pub address: [u8; 32],
    pub owner: [u8; 32],
    pub lamports: u64,
    pub signer: bool,
    pub writable: bool,
    pub executable: bool,
    pub borrow_state: u8,
}

impl Fields {
    /// An unborrowed, read-only, unsigned, non-executable account at `address` owned by `owner`.
    pub fn plain(address: [u8; 32], owner: [u8; 32]) -> Self {
        Self {
            address,
            owner,
            lamports: 0,
            signer: false,
            writable: false,
            executable: false,
            borrow_state: NOT_BORROWED,
        }
    }
}

impl<const DATA: usize> AccountMemory<DATA> {
    /// An account with `fields`, whose data is the first `data_len` bytes of `data`.
    pub fn new(fields: Fields, data: [u8; DATA], data_len: usize) -> Self {
        assert!(data_len <= DATA);
        Self {
            header: RuntimeAccount {
                borrow_state: fields.borrow_state,
                is_signer: u8::from(fields.signer),
                is_writable: u8::from(fields.writable),
                executable: u8::from(fields.executable),
                padding: [0; 4],
                address: Address::new_from_array(fields.address),
                owner: Address::new_from_array(fields.owner),
                lamports: fields.lamports,
                data_len: data_len as u64,
            },
            data,
        }
    }

    /// The account's view. The pointer is derived from the whole allocation, so the view may read
    /// past the header into the data.
    pub fn view(&mut self) -> AccountView {
        // SAFETY: `self` is a runtime header directly followed by at least `data_len` data bytes.
        unsafe { AccountView::new_unchecked((self as *mut Self).cast::<RuntimeAccount>()) }
    }
}
