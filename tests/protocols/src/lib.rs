//! The protocol-test harness: Ballista's live-protocol templates, run in LiteSVM as real signed
//! transactions against the real mainnet programs.
//!
//! - [`snapshot`] loads the committed one-slot mainnet snapshot, checks every hash, and builds a
//!   LiteSVM from it with Ballista built from source.
//! - [`wallet`] funds test wallets and writes their token balances.
//! - [`oracle`] moves a Pyth or a Scope price.
//! - [`orca`] opens Orca positions, adds liquidity and swaps through Orca's own instructions, and
//!   quotes fees with Orca's own math.
//! - [`template`] uploads a template with Ballista's own instructions and builds runs by the
//!   account and input names recorded in `fixtures/protocol-examples.json`.
//! - [`tx`] signs and sends, checks the wire size, and names the program that failed.
//! - [`kamino`] builds Kamino Lend's instructions and reads its reserves and obligations.
//! - [`marginfi`] builds marginfi's instructions and reads its banks and accounts.
//! - [`lending`] loads the lending snapshot and holds the setup the Kamino and marginfi scenarios
//!   share.
//!
//! Tests may write only three kinds of state directly: test wallets' SOL and token balances,
//! oracle prices, and the clock. Every other change goes through the protocols' own instructions.

pub mod kamino;
pub mod lending;
pub mod marginfi;
pub mod oracle;
pub mod orca;
pub mod snapshot;
pub mod template;
pub mod tx;
pub mod wallet;

/// Decodes a hex string such as a fixture payload. Surrounding whitespace is ignored.
///
/// # Panics
///
/// If the text has an odd number of digits or a character that is not a hex digit.
pub fn decode_hex(text: &str) -> Vec<u8> {
    let (pairs, odd) = text.trim().as_bytes().as_chunks::<2>();
    assert!(odd.is_empty(), "hex has an odd number of digits");
    let nibble = |digit: u8| char::from(digit).to_digit(16);
    pairs
        .iter()
        .map(|pair| match (nibble(pair[0]), nibble(pair[1])) {
            (Some(high), Some(low)) => (high << 4 | low) as u8,
            _ => panic!("{:?} is not a hex byte", String::from_utf8_lossy(pair)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use {
        super::{decode_hex, kamino, marginfi, oracle},
        sha2::{Digest, Sha256},
    };

    /// Every discriminator written out by hand in this crate is Anchor's: the first eight bytes of
    /// `sha256("global:<handler>")` for an instruction, `sha256("account:<Type>")` for an account.
    #[test]
    fn every_hand_written_discriminator_is_anchors() {
        for (preimage, written) in [
            ("global:marginfi_account_initialize", marginfi::INITIALIZE),
            ("global:lending_account_deposit", marginfi::DEPOSIT),
            ("global:lending_account_withdraw", marginfi::WITHDRAW),
            (
                "global:deposit_reserve_liquidity_and_obligation_collateral",
                kamino::DEPOSIT_V1,
            ),
            ("account:PriceUpdateV2", oracle::DISCRIMINATOR),
            ("account:OraclePrices", oracle::SCOPE_DISCRIMINATOR),
        ] {
            assert_eq!(Sha256::digest(preimage)[..8], written, "{preimage}");
        }
    }

    #[test]
    fn hex_decodes_and_ignores_surrounding_whitespace() {
        assert_eq!(decode_hex(" 00ff7A\n"), vec![0x00, 0xff, 0x7a]);
        assert_eq!(decode_hex(""), Vec::<u8>::new());
    }

    #[test]
    #[should_panic(expected = "odd number of digits")]
    fn hex_rejects_an_odd_length() {
        decode_hex("abc");
    }

    #[test]
    #[should_panic(expected = "\"+f\" is not a hex byte")]
    fn hex_rejects_a_sign() {
        decode_hex("+f");
    }
}
