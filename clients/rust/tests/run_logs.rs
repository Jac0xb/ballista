//! The log decoders on real logs: `tests/fixtures/run-logs.json`, recorded from the program in
//! Mollusk by `run_logs_for_the_sdk_decoders` in `tests/ballista`, which `UPDATE_FIXTURES=1`
//! rewrites.

use ballista_sdk::{program_data, BallistaOutput, LogError, RunEvent, ID};
use solana_program::pubkey::Pubkey;
use std::str::FromStr;

const FIXTURE: &str = include_str!("fixtures/run-logs.json");

/// The array of strings under `key`. The fixture is machine-written: the array closes on its own
/// line, and no log line holds a quote.
fn lines(key: &str) -> Vec<&'static str> {
    let marker = format!("\"{key}\": [");
    let start = FIXTURE.find(&marker).unwrap_or_else(|| panic!("no {key}")) + marker.len();
    let body = &FIXTURE[start..];
    body[..body.find("\n  ]").unwrap()]
        .split('"')
        .skip(1)
        .step_by(2)
        .collect()
}

/// The address under `key`.
fn address(key: &str) -> Pubkey {
    let marker = format!("\"{key}\": \"");
    let start = FIXTURE.find(&marker).unwrap_or_else(|| panic!("no {key}")) + marker.len();
    let end = start + FIXTURE[start..].find('"').unwrap();
    Pubkey::from_str(&FIXTURE[start..end]).unwrap()
}

/// An `EMIT` of `tag` and little-endian `u64` values.
fn emit(tag: &[u8; 4], values: &[u64]) -> Vec<u8> {
    let mut bytes = tag.to_vec();
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// An outer run logs `OUTR`, runs an inner template that logs `INNR` and its event, calls the
/// Token program, then logs `DONE` and its own event. Each line is attributed to its run by stack
/// height, and the events say which template ran and which invokes ran.
#[test]
fn a_nested_run_s_lines_are_told_from_the_outer_run_s() {
    let logs = lines("nested");
    assert_eq!(address("ballista"), ID);
    let found = program_data(&logs).unwrap();
    assert!(found.iter().all(|line| line.program == ID), "{found:?}");

    let outputs: Vec<(usize, BallistaOutput)> = found
        .iter()
        .map(|line| (line.height, line.ballista_output(&ID).unwrap()))
        .collect();
    let (outr, innr, done) = (
        emit(b"OUTR", &[1]),
        emit(b"INNR", &[7]),
        emit(b"DONE", &[7, 165]),
    );
    let event = |expanded, executed, template| RunEvent {
        version: 1,
        iterations: 0,
        expanded,
        executed,
        template,
    };
    assert_eq!(
        outputs,
        [
            (1, BallistaOutput::Emit(&outr)),
            (2, BallistaOutput::Emit(&innr)),
            (2, BallistaOutput::RunEvent(event(0, 0, address("inner")))),
            (1, BallistaOutput::Emit(&done)),
            // Two invokes reached, the nested run and the Token call, and both ran.
            (
                1,
                BallistaOutput::RunEvent(event(2, 0b11, address("outer")))
            ),
        ]
    );
}

/// The `Program return:` line names the program whose invocation ended, not the one that set the
/// bytes: the outer run sets no return data, so its line holds what the Token program returned.
#[test]
fn a_run_s_return_line_can_hold_another_program_s_bytes() {
    let logs = lines("nested");
    let token = address("token");
    let returns: Vec<&str> = logs
        .iter()
        .filter_map(|line| line.strip_prefix("Program return: "))
        .collect();
    assert_eq!(
        returns,
        [
            format!("{ID} BwAAAAAAAAA="),    // the inner run's 7
            format!("{token} pQAAAAAAAAA="), // the Token program's 165
            format!("{ID} pQAAAAAAAAA="),    // the same 165, under Ballista's name
        ]
    );
}

/// A failed run's lines are still logged, so a reader must check that the transaction succeeded.
#[test]
fn a_failed_run_still_logs_what_came_before_the_failure() {
    let logs = lines("failed");
    let found = program_data(&logs).unwrap();
    let outputs: Vec<_> = found.iter().map(|line| line.ballista_output(&ID)).collect();
    assert_eq!(outputs, [Some(BallistaOutput::Emit(&emit(b"TRY1", &[0])))]);
}

#[test]
fn cut_logs_are_reported_rather_than_read_in_part() {
    let logs = lines("nested");
    let mut cut = logs[..6].to_vec();
    cut.push("Log truncated");
    assert_eq!(program_data(&cut), Err(LogError::Truncated));
    assert_eq!(program_data(&logs[..6]), Err(LogError::Truncated));
}
