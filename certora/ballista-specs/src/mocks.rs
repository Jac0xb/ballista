//! Stand-ins for runtime syscalls the prover does not model.
//!
//! The prover treats a syscall it has no model for as a call that writes nothing. For
//! `sol_get_return_data` that includes its result register: the size the program reads back is
//! whatever `r0` held before the call, and the setter's address is the zeroes the caller put
//! there (`SolanaFunctions.kt` in the prover lists no output and no summary for it).
//!
//! The prover identifies a syscall by the name its call relocates to, before it looks for a
//! definition, so a function named `sol_get_return_data` would still read as the syscall. The
//! build script passes `--wrap=sol_get_return_data` to the SBF linker, which sends every call the
//! program makes to `__wrap_sol_get_return_data` below: an ordinary function the prover inlines.
//! Only the spec binary is linked this way; the program's own build never sees this crate.

#[cfg(target_os = "solana")]
mod return_data {
    use cvlr::nondet::nondet;

    /// `sol_get_return_data`: the return data the last CPI left, and the program that set it.
    ///
    /// The setter is any address and the reported size any length. The first 32 bytes of the data
    /// are the setter's own address; the rest of the caller's buffer is left as it was. Echoing the
    /// setter lets a rule see which program's data a successful read returned. It hides no run of
    /// Ballista's provenance check, which compares the setter with the program it invoked before
    /// it reads any data byte.
    ///
    /// Every write is one eight-byte store, the width the program reads both buffers at.
    #[no_mangle]
    pub unsafe extern "C" fn __wrap_sol_get_return_data(
        data: *mut u8,
        length: u64,
        program_id: *mut u8,
    ) -> u64 {
        let setter: [u64; 4] = [nondet(), nondet(), nondet(), nondet()];
        let id = program_id.cast::<u64>();
        id.write_unaligned(setter[0]);
        id.add(1).write_unaligned(setter[1]);
        id.add(2).write_unaligned(setter[2]);
        id.add(3).write_unaligned(setter[3]);
        if length >= 32 {
            let words = data.cast::<u64>();
            words.write_unaligned(setter[0]);
            words.add(1).write_unaligned(setter[1]);
            words.add(2).write_unaligned(setter[2]);
            words.add(3).write_unaligned(setter[3]);
        }
        nondet()
    }
}
