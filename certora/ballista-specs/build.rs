//! Routes the program's `sol_get_return_data` calls to the stand-in in `src/mocks.rs`, in the SBF
//! build only. The prover recognises a call by its relocation's symbol name and treats a known
//! syscall name as the syscall even when the binary defines a function of that name, so the stand-in
//! needs a name of its own: `--wrap` resolves every undefined reference to `sol_get_return_data` to
//! `__wrap_sol_get_return_data` instead.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("solana") {
        println!("cargo:rustc-link-arg-cdylib=--wrap=sol_get_return_data");
    }
}
