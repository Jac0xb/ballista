//! Pyth prices. Write rule 2 lets tests move an oracle price directly, in the account's real
//! layout; nothing else about the account is written. [`copy_pyth_feed`] writes a second price
//! account, for another feed, under the same rule.
//!
//! A push-oracle `PriceUpdateV2` (owner: the Pyth receiver, 134 bytes) is always fully verified,
//! and its offsets below assume that: a partially verified account has an extra byte at 41 and
//! every later field one byte further on.

use {litesvm::LiteSVM, solana_account::Account, solana_address::Address};

/// The Pyth Solana receiver, which owns every `PriceUpdateV2`.
pub const PYTH_RECEIVER: Address =
    Address::from_str_const("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");
/// The Pyth push oracle. It keeps each sponsored feed at a program-derived address, one per shard
/// (see [`pyth_feed_address`]), and signs that feed's updates with it.
pub const PYTH_PUSH_ORACLE: Address =
    Address::from_str_const("pythWSnswVUd12oZpeFP8e9CVaEqJg25g1Vtc2biRsT");
/// SOL/USD's feed id, which the snapshot's `pythSolUsd` carries.
pub const SOL_USD_FEED_ID: Address =
    hex_feed_id("ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d");
/// USDC/USD's feed id.
pub const USDC_USD_FEED_ID: Address =
    hex_feed_id("eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a");
/// `PriceUpdateV2::LEN`.
pub const PRICE_UPDATE_LEN: usize = 134;
/// Anchor's `account:PriceUpdateV2` discriminator.
const DISCRIMINATOR: [u8; 8] = [0x22, 0xf1, 0x23, 0x63, 0x9d, 0x7e, 0xf4, 0xcd];
/// `write_authority`: for a push-oracle feed, the feed's own address.
const WRITE_AUTHORITY: usize = 8;
/// `verification_level`, a Borsh enum tag: `Partial` is 0, `Full` is 1.
const VERIFICATION_LEVEL: usize = 40;
const FULL: u8 = 1;
const FEED_ID: usize = 41;
const PRICE: usize = 73;
const CONF: usize = 81;
const EXPONENT: usize = 89;
const PUBLISH_TIME: usize = 93;

/// The fields of a price a test may move.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PythPrice {
    pub price: i64,
    pub conf: u64,
    pub exponent: i32,
    pub publish_time: i64,
}

/// Reads a `PriceUpdateV2`'s price, confidence, exponent and publish time.
///
/// # Panics
///
/// If `feed` is not a fully verified `PriceUpdateV2`.
pub fn pyth_price(svm: &LiteSVM, feed: &Address) -> PythPrice {
    let data = price_update(svm, feed).data;
    PythPrice {
        price: i64::from_le_bytes(field(&data, PRICE)),
        conf: u64::from_le_bytes(field(&data, CONF)),
        exponent: i32::from_le_bytes(field(&data, EXPONENT)),
        publish_time: i64::from_le_bytes(field(&data, PUBLISH_TIME)),
    }
}

/// The feed id a `PriceUpdateV2` carries (SOL/USD is `ef0d8b6f…`), as the 32 bytes a template
/// compares with a `pubkey` input.
///
/// # Panics
///
/// If `feed` is not a fully verified `PriceUpdateV2`.
pub fn pyth_feed_id(svm: &LiteSVM, feed: &Address) -> Address {
    Address::new_from_array(field(&price_update(svm, feed).data, FEED_ID))
}

/// Writes a `PriceUpdateV2`'s price, confidence, exponent and publish time, and nothing else.
///
/// To move one field, update what [`pyth_price`] read:
///
/// ```text
/// let market = pyth_price(&svm, &feed);
/// set_pyth_price(&mut svm, &feed, PythPrice { price: market.price * 105 / 100, ..market });
/// ```
///
/// # Panics
///
/// If `feed` is not a fully verified `PriceUpdateV2`.
pub fn set_pyth_price(svm: &mut LiteSVM, feed: &Address, price: PythPrice) {
    let mut account = price_update(svm, feed);
    account.data[PRICE..PRICE + 8].copy_from_slice(&price.price.to_le_bytes());
    account.data[CONF..CONF + 8].copy_from_slice(&price.conf.to_le_bytes());
    account.data[EXPONENT..EXPONENT + 4].copy_from_slice(&price.exponent.to_le_bytes());
    account.data[PUBLISH_TIME..PUBLISH_TIME + 8].copy_from_slice(&price.publish_time.to_le_bytes());
    svm.set_account(*feed, account)
        .unwrap_or_else(|error| panic!("writing price update {feed} failed: {error:?}"));
}

/// Where the push oracle keeps `feed_id`'s price on `shard`: the program-derived address of
/// `[shard as a little-endian u16, feed id]`. SOL/USD on shard 0 is the snapshot's `pythSolUsd`.
pub fn pyth_feed_address(feed_id: &Address, shard: u16) -> Address {
    Address::find_program_address(&[&shard.to_le_bytes(), feed_id.as_ref()], &PYTH_PUSH_ORACLE).0
}

/// Writes a second price account for another feed, and returns its address: the `PriceUpdateV2`
/// at `source`, copied to where the push oracle keeps `feed_id` on shard 0.
///
/// The copy differs from `source` only in its feed id and its write authority, which for a
/// push-oracle feed is the feed's own address. Its price, confidence, exponent and publish time are
/// `source`'s until [`set_pyth_price`] moves them.
///
/// This is write rule 2 as well: the account is an oracle price account in its real layout, owned
/// by the receiver and fully verified like every other. Refreshing the snapshot to take the feed
/// from mainnet would move every other test's state with it.
///
/// # Panics
///
/// If `source` is not a fully verified `PriceUpdateV2`, or the SVM already holds an account at the
/// new address.
pub fn copy_pyth_feed(svm: &mut LiteSVM, source: &Address, feed_id: &Address) -> Address {
    let mut account = price_update(svm, source);
    let address = pyth_feed_address(feed_id, 0);
    assert!(
        svm.get_account(&address).is_none(),
        "{address} already holds an account; copying a feed never overwrites one"
    );
    account.data[WRITE_AUTHORITY..WRITE_AUTHORITY + 32].copy_from_slice(address.as_ref());
    account.data[FEED_ID..FEED_ID + 32].copy_from_slice(feed_id.as_ref());
    svm.set_account(address, account)
        .unwrap_or_else(|error| panic!("writing price update {address} failed: {error:?}"));
    address
}

/// A feed id from the hex Pyth publishes it in.
const fn hex_feed_id(hex: &str) -> Address {
    let digits = hex.as_bytes();
    assert!(digits.len() == 64, "a feed id is 32 bytes, 64 hex digits");
    let mut bytes = [0; 32];
    let mut index = 0;
    while index < 32 {
        bytes[index] = hex_digit(digits[2 * index]) << 4 | hex_digit(digits[2 * index + 1]);
        index += 1;
    }
    Address::new_from_array(bytes)
}

const fn hex_digit(digit: u8) -> u8 {
    match digit {
        b'0'..=b'9' => digit - b'0',
        b'a'..=b'f' => digit - b'a' + 10,
        _ => panic!("a feed id is lowercase hex"),
    }
}

/// The account at `feed`, checked to be a `PriceUpdateV2` whose offsets are the `Full` ones.
fn price_update(svm: &LiteSVM, feed: &Address) -> Account {
    let account = svm
        .get_account(feed)
        .unwrap_or_else(|| panic!("price update {feed} is not in the SVM"));
    assert!(
        account.owner == PYTH_RECEIVER
            && account.data.len() == PRICE_UPDATE_LEN
            && account.data[..8] == DISCRIMINATOR,
        "{feed} is not a Pyth PriceUpdateV2"
    );
    assert_eq!(
        account.data[VERIFICATION_LEVEL], FULL,
        "{feed} is not fully verified, so its fields are not at the offsets this writes"
    );
    account
}

fn field<const N: usize>(data: &[u8], offset: usize) -> [u8; N] {
    data[offset..offset + N].try_into().unwrap()
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::snapshot::{Snapshot, SNAPSHOT_DIR},
    };

    const SOL_USD: Address =
        Address::from_str_const("7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE");

    /// The snapshot's SOL/USD account alone, in a bare SVM, and the snapshot's time.
    fn sol_usd() -> (LiteSVM, Account, i64) {
        let snapshot = Snapshot::load(SNAPSHOT_DIR);
        let account = snapshot
            .account(&SOL_USD)
            .expect("the snapshot holds the SOL/USD price update")
            .clone();
        let mut svm = LiteSVM::new();
        svm.set_account(SOL_USD, account.clone()).unwrap();
        (svm, account, snapshot.clock.unix_timestamp)
    }

    #[test]
    fn a_written_price_reads_back_and_nothing_else_changes() {
        let (mut svm, before, now) = sol_usd();
        // Loose enough to hold at any refresh, tight enough to catch two fields read from each
        // other's offsets.
        let snapshotted = pyth_price(&svm, &SOL_USD);
        assert_eq!(snapshotted.exponent, -8);
        assert!(
            (100_000_000..1_000_000_000_000).contains(&snapshotted.price),
            "SOL between $1 and $10,000: {snapshotted:?}"
        );
        assert!(
            snapshotted.conf < snapshotted.price.unsigned_abs() / 100,
            "{snapshotted:?}"
        );
        assert!(
            (now - 600..=now).contains(&snapshotted.publish_time),
            "published within ten minutes before the snapshot: {snapshotted:?}"
        );
        assert_eq!(
            pyth_feed_id(&svm, &SOL_USD),
            Address::new_from_array(
                crate::decode_hex(
                    "ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d"
                )
                .try_into()
                .unwrap()
            )
        );

        let moved = PythPrice {
            price: -12_345_678_901,
            conf: u64::MAX - 1,
            exponent: -5,
            publish_time: snapshotted.publish_time + 3_600,
        };
        set_pyth_price(&mut svm, &SOL_USD, moved);
        assert_eq!(pyth_price(&svm, &SOL_USD), moved);

        // One field at a time, the others as read.
        let raised = PythPrice {
            price: snapshotted.price * 105 / 100,
            ..pyth_price(&svm, &SOL_USD)
        };
        set_pyth_price(&mut svm, &SOL_USD, raised);
        assert_eq!(
            pyth_price(&svm, &SOL_USD),
            PythPrice {
                price: snapshotted.price * 105 / 100,
                ..moved
            }
        );

        let after = svm.get_account(&SOL_USD).unwrap();
        assert_eq!(
            (after.owner, after.lamports, after.data.len()),
            (before.owner, before.lamports, before.data.len())
        );
        let changed: Vec<usize> = (0..PRICE_UPDATE_LEN)
            .filter(|&at| after.data[at] != before.data[at])
            .collect();
        assert!(
            changed
                .iter()
                .all(|&at| (PRICE..PUBLISH_TIME + 8).contains(&at)),
            "bytes outside price..publish_time changed: {changed:?}"
        );
    }

    #[test]
    #[should_panic(expected = "is not fully verified")]
    fn a_partially_verified_update_is_refused() {
        let (mut svm, mut account, _) = sol_usd();
        account.data[VERIFICATION_LEVEL] = 0;
        svm.set_account(SOL_USD, account).unwrap();
        let price = PythPrice {
            price: 1,
            conf: 1,
            exponent: -8,
            publish_time: 1,
        };
        set_pyth_price(&mut svm, &SOL_USD, price);
    }

    /// The copy is mainnet's USDC/USD feed in all but its price: the address the push oracle keeps
    /// it at, that feed's id, and itself as write authority. Nothing else differs from the source,
    /// and the source is left as it was.
    #[test]
    fn a_copied_feed_differs_only_in_its_feed_id_and_write_authority() {
        let (mut svm, source, _) = sol_usd();
        assert_eq!(pyth_feed_id(&svm, &SOL_USD), SOL_USD_FEED_ID);
        assert_eq!(pyth_feed_address(&SOL_USD_FEED_ID, 0), SOL_USD);
        assert_eq!(
            source.data[WRITE_AUTHORITY..VERIFICATION_LEVEL],
            *SOL_USD.as_ref()
        );

        let copy = copy_pyth_feed(&mut svm, &SOL_USD, &USDC_USD_FEED_ID);
        assert_eq!(
            copy,
            Address::from_str_const("Dpw1EAVrSB1ibxiDQyTAW6Zip3J4Btk2x4SgApQCeFbX")
        );
        assert_eq!(pyth_feed_id(&svm, &copy), USDC_USD_FEED_ID);
        assert_eq!(pyth_price(&svm, &copy), pyth_price(&svm, &SOL_USD));
        let copied = svm.get_account(&copy).unwrap();
        assert_eq!(
            (
                copied.owner,
                copied.lamports,
                copied.executable,
                copied.rent_epoch
            ),
            (
                source.owner,
                source.lamports,
                source.executable,
                source.rent_epoch
            )
        );
        assert_eq!(
            copied.data[WRITE_AUTHORITY..VERIFICATION_LEVEL],
            *copy.as_ref()
        );
        let changed: Vec<usize> = (0..PRICE_UPDATE_LEN)
            .filter(|&at| copied.data[at] != source.data[at])
            .collect();
        assert!(
            changed
                .iter()
                .all(|&at| (WRITE_AUTHORITY..VERIFICATION_LEVEL).contains(&at)
                    || (FEED_ID..PRICE).contains(&at)),
            "bytes outside the write authority and the feed id changed: {changed:?}"
        );
        assert_eq!(svm.get_account(&SOL_USD), Some(source));
    }

    #[test]
    #[should_panic(expected = "already holds an account; copying a feed never overwrites one")]
    fn a_copy_never_overwrites_an_account() {
        let (mut svm, _, _) = sol_usd();
        copy_pyth_feed(&mut svm, &SOL_USD, &SOL_USD_FEED_ID);
    }
}
