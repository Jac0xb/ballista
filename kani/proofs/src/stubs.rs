//! Stubs for functions whose real bodies are out of the solver's reach, used with
//! `#[kani::stub]` (run with `-Z stubbing`). Each returns any value the real function could.

// Referenced only from `#[kani::stub]` attributes, which rustc's dead-code pass does not see.
#![allow(dead_code)]

use pinocchio::Address;

/// `pda::try_find_program_address`: any address and bump, or none.
pub fn try_find_program_address(_seeds: &[&[u8]], _program_id: &Address) -> Option<(Address, u8)> {
    if kani::any() {
        Some((Address::new_from_array(kani::any()), kani::any()))
    } else {
        None
    }
}

/// `pda::create_program_address`: any address, or none.
pub fn create_program_address(_seeds: &[&[u8]], _program_id: &Address) -> Option<Address> {
    if kani::any() {
        Some(Address::new_from_array(kani::any()))
    } else {
        None
    }
}
