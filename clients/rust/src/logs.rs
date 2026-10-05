//! A run's output in a transaction's logs: which program logged each `Program data:` line, and
//! whether one of Ballista's lines is a run event or a template's `EMIT`. The TypeScript SDK's
//! `decodeRunEvent` and `parseProgramData` read the same logs the same way.

use ballista_common::template::{MIN_EMIT_TAG_LEN, RUN_EVENT_TAG_FAMILY};
use solana_program::pubkey::Pubkey;
use std::{fmt, str::FromStr};

/// The run event's length: `BEV1`, version, iterations, expanded, executed, template address.
pub const RUN_EVENT_LEN: usize = 4 + 1 + 1 + 1 + 8 + 32;
/// The run event's first four bytes. The first three are the tag family no `EMIT` may start with.
const RUN_EVENT_MAGIC: [u8; 4] = *b"BEV1";

/// The record a run of a template with `PROGRAM_FLAG_EMIT_EVENT` logs when it succeeds, as one
/// `Program data:` line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunEvent {
    /// The template's bytecode version, 1.
    pub version: u8,
    /// The batch rows the run was given, up to 255. A `repeat` loop's passes are not counted.
    pub iterations: u8,
    /// The invokes the run reached, counting each loop pass.
    pub expanded: u8,
    /// One bit per invoke reached, in order from bit 0: set when it ran, clear when its condition
    /// skipped it.
    pub executed: u64,
    /// The template account that ran.
    pub template_address: Pubkey,
}

/// Decodes a run event from one `Program data:` field: exactly [`RUN_EVENT_LEN`] bytes that start
/// with `BEV1`. Anything else gives `None`, including a template's `EMIT`, whose tag cannot start
/// with `BEV`. The version byte is not checked.
///
/// It does not know which program logged the bytes, and any program can log these. Take the field
/// from a line [`program_data`] attributes to Ballista.
pub fn decode_run_event(data: &[u8]) -> Option<RunEvent> {
    if data.len() != RUN_EVENT_LEN || !data.starts_with(&RUN_EVENT_MAGIC) {
        return None;
    }
    Some(RunEvent {
        version: data[4],
        iterations: data[5],
        expanded: data[6],
        executed: u64::from_le_bytes(data[7..15].try_into().ok()?),
        template_address: Pubkey::new_from_array(data[15..].try_into().ok()?),
    })
}

/// One `Program data:` line of a transaction's logs, and the program that logged it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgramDataLine {
    /// The program that logged the line: the innermost invocation open around it.
    pub program: Pubkey,
    /// That invocation's stack height: 1 for one of the transaction's instructions, 2 for a program
    /// it calls, such as a nested Ballista run, and so on.
    pub height: usize,
    /// The invocation's number, counting the transaction's invocations from 0 in log order. Lines
    /// with the same number came from the same call: a template's `EMIT` lines and the run event
    /// that names the template, say.
    pub invocation: usize,
    /// The line's fields, decoded from base64. `sol_log_data` logs each slice as one field, and
    /// Ballista logs one.
    pub fields: Vec<Vec<u8>>,
}

/// What one of Ballista's `Program data:` lines holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BallistaOutput<'a> {
    /// The run event of a template that sets `PROGRAM_FLAG_EMIT_EVENT`.
    RunEvent(RunEvent),
    /// A template's `EMIT`: its literal tag, then its encoded parts.
    Emit(&'a [u8]),
}

impl ProgramDataLine {
    /// What the line holds, if the deployment at `program_id` logged it, told apart by the tag
    /// rule: an `EMIT` starts with a tag of at least 4 bytes that does not start with `BEV`, and
    /// the run event starts with `BEV1`.
    ///
    /// `None` for a line another program logged, even one holding the same bytes, and for a
    /// Ballista line that is neither, such as a later version's run event.
    pub fn ballista_output(&self, program_id: &Pubkey) -> Option<BallistaOutput<'_>> {
        let [field] = self.fields.as_slice() else {
            return None;
        };
        if self.program != *program_id {
            None
        } else if field.starts_with(&RUN_EVENT_TAG_FAMILY) {
            decode_run_event(field).map(BallistaOutput::RunEvent)
        } else if field.len() >= MIN_EMIT_TAG_LEN {
            Some(BallistaOutput::Emit(field))
        } else {
            None
        }
    }
}

/// Logs that do not nest, which [`program_data`] refuses. Each variant holds the index of the line
/// that does not fit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogError {
    /// A `Program data:` line outside every invocation.
    OutsideEveryInvocation(usize),
    /// An `invoke` line whose stack height skips a level.
    SkipsALevel(usize),
    /// A `success` or `failed` line for a program other than the innermost one open.
    NotTheInnermost(usize),
    /// A `Program data:` field that is not base64.
    NotBase64(usize),
}

impl fmt::Display for LogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogError::OutsideEveryInvocation(line) => {
                write!(formatter, "log line {line} is outside every invocation")
            }
            LogError::SkipsALevel(line) => write!(formatter, "log line {line} skips a level"),
            LogError::NotTheInnermost(line) => write!(
                formatter,
                "log line {line} ends an invocation that is not the innermost open one"
            ),
            LogError::NotBase64(line) => write!(formatter, "log line {line} is not base64"),
        }
    }
}

impl std::error::Error for LogError {}

/// Every `Program data:` line in a transaction's logs, in order, with the program that logged it.
///
/// A `Program data:` line does not name its program. The invocation open around it does: a
/// `Program <address> invoke [<height>]` line opens one, and `Program <address> success` or
/// `Program <address> failed: ...` closes it. So the lines of a nested run, and of the programs a
/// run calls, are told apart from the outer run's.
///
/// Logs cut off at Solana's log limit still parse, up to the `Log truncated` line; check for that
/// line when you need every event.
pub fn program_data<S: AsRef<str>>(logs: &[S]) -> Result<Vec<ProgramDataLine>, LogError> {
    // Each open invocation's program and number.
    let mut stack: Vec<(Pubkey, usize)> = Vec::new();
    let mut invocations = 0;
    let mut lines = Vec::new();
    for (index, line) in logs.iter().enumerate() {
        let Some(rest) = line.as_ref().strip_prefix("Program ") else {
            continue;
        };
        if let Some(fields) = rest.strip_prefix("data: ") {
            let &(program, invocation) = stack
                .last()
                .ok_or(LogError::OutsideEveryInvocation(index))?;
            let fields = fields
                .split(' ')
                .map(decode_base64)
                .collect::<Option<_>>()
                .ok_or(LogError::NotBase64(index))?;
            lines.push(ProgramDataLine {
                program,
                height: stack.len(),
                invocation,
                fields,
            });
            continue;
        }
        // `Program log: ...`, `Program return: ...` and the like name no program here.
        let Some((program, event)) = rest.split_once(' ') else {
            continue;
        };
        let Ok(program) = Pubkey::from_str(program) else {
            continue;
        };
        if let Some(height) = event.strip_prefix("invoke [") {
            let height = height
                .strip_suffix(']')
                .and_then(|height| height.parse().ok());
            if height != Some(stack.len() + 1) {
                return Err(LogError::SkipsALevel(index));
            }
            stack.push((program, invocations));
            invocations += 1;
        } else if event == "success" || event.starts_with("failed: ") {
            if stack.last().map(|&(open, _)| open) != Some(program) {
                return Err(LogError::NotTheInnermost(index));
            }
            stack.pop();
        }
    }
    Ok(lines)
}

/// Decodes a field as the TypeScript SDK's `atob` does (WHATWG forgiving-base64): whitespace is
/// ignored, and the padding is optional. The runtime always writes padded standard base64.
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let mut text: Vec<u8> = text
        .bytes()
        .filter(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\x0c' | b'\r'))
        .collect();
    if text.len().is_multiple_of(4) {
        let padding = text
            .iter()
            .rev()
            .take(2)
            .take_while(|&&byte| byte == b'=')
            .count();
        text.truncate(text.len() - padding);
    }
    if text.len() % 4 == 1 {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() * 3 / 4);
    let (mut buffer, mut bits) = (0u32, 0);
    for byte in text {
        buffer = buffer << 6 | u32::from(sextet(byte)?);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(bytes)
}

fn sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BALLISTA: &str = "BLSTAmUBA29tcRUvoq5DBYxRhGptrnWPtfQW65RszRWR";
    const JUPITER: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";

    fn key(text: &str) -> Pubkey {
        Pubkey::from_str(text).unwrap()
    }

    fn event_bytes(template: Pubkey) -> Vec<u8> {
        let mut event = b"BEV1".to_vec();
        event.extend_from_slice(&[1, 2, 3]);
        event.extend_from_slice(&0b101u64.to_le_bytes());
        event.extend_from_slice(template.as_ref());
        event
    }

    /// The same results as `atob`, checked in Node.
    #[test]
    fn base64_decodes_as_atob_does() {
        let cases: [(&str, &[u8]); 9] = [
            ("", b""),
            ("QQ==", b"A"),
            ("QQ", b"A"),
            ("QUI", b"AB"),
            ("QUJD\n", b"ABC"),
            (" QQ== ", b"A"),
            ("+/8=", &[0xfb, 0xff]),
            ("T1VUUgEAAAAAAAAA", b"OUTR\x01\0\0\0\0\0\0\0"),
            ("QUI=", b"AB"),
        ];
        for (text, bytes) in cases {
            assert_eq!(decode_base64(text).as_deref(), Some(bytes), "{text:?}");
        }
        for text in ["Q", "QQ=", "Q===", "QQ==QQ==", "Q=QQ", "!!!"] {
            assert_eq!(decode_base64(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_run_event_is_47_bytes_starting_with_bev1() {
        let template_address = Pubkey::new_from_array([7; 32]);
        let event = event_bytes(template_address);
        assert_eq!(
            decode_run_event(&event),
            Some(RunEvent {
                version: 1,
                iterations: 2,
                expanded: 3,
                executed: 0b101,
                template_address,
            })
        );
        assert_eq!(decode_run_event(&event[..46]), None, "short");
        let long = [event.as_slice(), &[0]].concat();
        assert_eq!(decode_run_event(&long), None, "long");
        let mut later = event.clone();
        later[3] = b'2';
        assert_eq!(decode_run_event(&later), None, "another event version");
        let mut version = event.clone();
        version[4] = 9;
        assert!(
            decode_run_event(&version).is_some(),
            "the version byte is not checked"
        );
        assert_eq!(decode_run_event(b"PAID"), None);
    }

    /// The lines of a nested run and of a program it calls sit between the outer run's own, and
    /// another program can log bytes that read as a run event. The same logs as the TypeScript
    /// SDK's test, with Jupiter's line replaced by a copy of a run event.
    #[test]
    fn each_data_line_belongs_to_the_invocation_around_it() {
        // `event_bytes` for a template of sevens, in base64.
        let event = "QkVWMQECAwUAAAAAAAAABwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=";
        let logs = [
            format!("Program {BALLISTA} invoke [1]"),
            format!("Program {BALLISTA} invoke [2]"),
            format!("Program {JUPITER} invoke [3]"),
            format!("Program data: {event}"),
            format!("Program {JUPITER} success"),
            "Program data: SU5OUgcAAAAAAAAA".to_owned(),
            "Program log: data: not a data line".to_owned(),
            format!("Program return: {BALLISTA} BwAAAAAAAAA="),
            format!("Program {BALLISTA} consumed 1466 of 1397452 compute units"),
            format!("Program {BALLISTA} success"),
            format!("Program data: {event}"),
            format!("Program {BALLISTA} success"),
        ];
        let lines = program_data(&logs).unwrap();
        let attributed: Vec<(Pubkey, usize, usize)> = lines
            .iter()
            .map(|line| (line.program, line.height, line.invocation))
            .collect();
        assert_eq!(
            attributed,
            [
                (key(JUPITER), 3, 2),
                (key(BALLISTA), 2, 1),
                (key(BALLISTA), 1, 0)
            ]
        );
        let ballista = key(BALLISTA);
        let outputs: Vec<_> = lines
            .iter()
            .map(|line| line.ballista_output(&ballista))
            .collect();
        let run_event = decode_run_event(&event_bytes(Pubkey::new_from_array([7; 32]))).unwrap();
        assert_eq!(
            outputs,
            [
                None,
                Some(BallistaOutput::Emit(b"INNR\x07\0\0\0\0\0\0\0")),
                Some(BallistaOutput::RunEvent(run_event)),
            ],
            "Jupiter's copy of the event is not Ballista's"
        );
    }

    /// Two runs in one transaction: each line carries its run's invocation number, so an `EMIT`
    /// pairs with the run event that names its template.
    #[test]
    fn invocations_are_numbered_in_log_order() {
        let logs = [
            format!("Program {BALLISTA} invoke [1]"),
            "Program data: UEFJRA==".to_owned(),
            "Program data: QkVWMQECAwUAAAAAAAAABwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc="
                .to_owned(),
            format!("Program {BALLISTA} success"),
            format!("Program {BALLISTA} invoke [1]"),
            "Program data: UEFJRA==".to_owned(),
            format!("Program {BALLISTA} success"),
        ];
        let lines = program_data(&logs).unwrap();
        let invocations: Vec<usize> = lines.iter().map(|line| line.invocation).collect();
        assert_eq!(invocations, [0, 0, 1]);
        let event = decode_run_event(&lines[1].fields[0]).unwrap();
        assert_eq!(event.template_address, Pubkey::new_from_array([7; 32]));
    }

    #[test]
    fn logs_that_do_not_nest_are_refused() {
        let invoke = |height: usize| format!("Program {BALLISTA} invoke [{height}]");
        let jupiter_success = format!("Program {JUPITER} success");
        let cases: [(Vec<String>, LogError); 6] = [
            (
                vec!["Program data: AQI=".into()],
                LogError::OutsideEveryInvocation(0),
            ),
            (vec![invoke(2)], LogError::SkipsALevel(0)),
            (vec![invoke(1), invoke(3)], LogError::SkipsALevel(1)),
            (
                vec![invoke(1), jupiter_success.clone()],
                LogError::NotTheInnermost(1),
            ),
            (vec![jupiter_success], LogError::NotTheInnermost(0)),
            (
                vec![invoke(1), "Program data: !!!".into()],
                LogError::NotBase64(1),
            ),
        ];
        for (logs, error) in cases {
            assert_eq!(program_data(&logs), Err(error), "{logs:?}");
        }
    }

    /// Logs cut at Solana's limit parse up to the cut, a failed invocation closes like a
    /// successful one, and a data line with no slices still has its one empty field.
    #[test]
    fn cut_failed_and_empty_logs_still_parse() {
        let truncated = [
            format!("Program {BALLISTA} invoke [1]"),
            "Program data: UEFJRA==".to_owned(),
            format!("Program {JUPITER} invoke [2]"),
            "Log truncated".to_owned(),
        ];
        let paid = ProgramDataLine {
            program: key(BALLISTA),
            height: 1,
            invocation: 0,
            fields: vec![b"PAID".to_vec()],
        };
        assert_eq!(program_data(&truncated), Ok(vec![paid]));

        let failed = [
            format!("Program {JUPITER} invoke [1]"),
            "Program data: AQI=".to_owned(),
            format!("Program {JUPITER} failed: custom program error: 0x1"),
            format!("Program {BALLISTA} invoke [1]"),
            "Program data: ".to_owned(),
            format!("Program {BALLISTA} success"),
        ];
        let lines = program_data(&failed).unwrap();
        let shapes: Vec<(Pubkey, usize, Vec<Vec<u8>>)> = lines
            .into_iter()
            .map(|line| (line.program, line.invocation, line.fields))
            .collect();
        assert_eq!(
            shapes,
            [
                (key(JUPITER), 0, vec![vec![1, 2]]),
                (key(BALLISTA), 1, vec![Vec::new()]),
            ]
        );
    }

    #[test]
    fn a_ballista_line_is_an_event_an_emit_or_neither() {
        let ballista = key(BALLISTA);
        let line = |fields: &[&[u8]]| ProgramDataLine {
            program: ballista,
            height: 1,
            invocation: 0,
            fields: fields.iter().map(|field| field.to_vec()).collect(),
        };
        assert_eq!(
            line(&[b"PAID"]).ballista_output(&ballista),
            Some(BallistaOutput::Emit(b"PAID"))
        );
        assert_eq!(
            line(&[b"PAI"]).ballista_output(&ballista),
            None,
            "shorter than any tag"
        );
        assert_eq!(
            line(&[b"BEV2 later"]).ballista_output(&ballista),
            None,
            "the event family"
        );
        assert_eq!(
            line(&[b"PAID", b"PAID"]).ballista_output(&ballista),
            None,
            "two fields"
        );
        assert_eq!(line(&[]).ballista_output(&ballista), None);
        assert_eq!(
            line(&[b"PAID"]).ballista_output(&key(JUPITER)),
            None,
            "another deployment"
        );
    }
}
