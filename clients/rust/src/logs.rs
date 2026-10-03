//! A run's output in a transaction's logs: which program logged each `Program data:` line, and
//! whether one of Ballista's lines is a run event or a template's `EMIT`.

use ballista_common::template::{MIN_EMIT_TAG_LEN, RUN_EVENT_TAG_FAMILY};
use solana_program::pubkey::Pubkey;
use std::{fmt, str::FromStr};

/// The run event's first four bytes. The first three are the tag family no `EMIT` may start with.
const RUN_EVENT_MAGIC: [u8; 4] = *b"BEV1";
/// The run event's length: magic, version, iterations, expanded, executed, template address.
const RUN_EVENT_LEN: usize = 4 + 1 + 1 + 1 + 8 + 32;

/// The record a run of a template with `PROGRAM_FLAG_EMIT_EVENT` logs when it succeeds, as one
/// `Program data:` line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunEvent {
    /// The template's bytecode version, 1.
    pub version: u8,
    /// The batch rows the run was given. A `repeat` loop's passes are not counted.
    pub iterations: u8,
    /// The invokes the run reached, counting each loop pass.
    pub expanded: u8,
    /// One bit per invoke reached, in order from bit 0: set when it ran, clear when its condition
    /// skipped it.
    pub executed: u64,
    /// The template that ran.
    pub template: Pubkey,
}

/// Decodes a run event from the bytes of one `Program data:` field: 47 bytes that start with
/// `BEV1`. Anything else gives `None`, including a template's `EMIT` output, whose tag never
/// starts with `BEV`.
///
/// The bytes alone prove nothing: only a line Ballista logged holds a run event, and a
/// `Program data:` line does not name its program. [`program_data`] finds which program logged
/// each line.
pub fn decode_run_event(data: &[u8]) -> Option<RunEvent> {
    if data.len() != RUN_EVENT_LEN || !data.starts_with(&RUN_EVENT_MAGIC) {
        return None;
    }
    Some(RunEvent {
        version: data[4],
        iterations: data[5],
        expanded: data[6],
        executed: u64::from_le_bytes(data[7..15].try_into().ok()?),
        template: Pubkey::new_from_array(data[15..].try_into().ok()?),
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
    /// The line's fields, decoded from base64. Ballista logs one per line.
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

/// Why [`program_data`] could not attribute a transaction's logs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogError {
    /// The logs stop inside an invocation, as they do when the runtime reaches its log limit and
    /// writes `Log truncated`. Lines after the cut, run events among them, are missing.
    Truncated,
    /// The line at this index does not fit the invocations around it: a `Program data:` line
    /// outside every invocation, an `invoke` that skips a stack height, a `success` or `failed`
    /// line for a program other than the innermost one, or a field that is not base64.
    Malformed(usize),
}

impl fmt::Display for LogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogError::Truncated => formatter.write_str("the logs stop inside an invocation"),
            LogError::Malformed(index) => {
                write!(
                    formatter,
                    "log line {index} does not fit the invocations around it"
                )
            }
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
pub fn program_data<S: AsRef<str>>(logs: &[S]) -> Result<Vec<ProgramDataLine>, LogError> {
    let mut stack: Vec<Pubkey> = Vec::new();
    let mut lines = Vec::new();
    for (index, line) in logs.iter().enumerate() {
        let line = line.as_ref();
        if line == "Log truncated" {
            return Err(LogError::Truncated);
        }
        let Some(rest) = line.strip_prefix("Program ") else {
            continue;
        };
        let malformed = LogError::Malformed(index);
        if let Some(fields) = rest.strip_prefix("data: ") {
            let program = *stack.last().ok_or(malformed)?;
            let fields = match fields {
                "" => Vec::new(),
                fields => fields
                    .split(' ')
                    .map(decode_base64)
                    .collect::<Option<_>>()
                    .ok_or(malformed)?,
            };
            lines.push(ProgramDataLine {
                program,
                height: stack.len(),
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
            if height
                .strip_suffix(']')
                .and_then(|height| height.parse().ok())
                != Some(stack.len() + 1)
            {
                return Err(malformed);
            }
            stack.push(program);
        } else if (event == "success" || event.starts_with("failed: "))
            && stack.pop() != Some(program)
        {
            return Err(malformed);
        }
    }
    if stack.is_empty() {
        Ok(lines)
    } else {
        Err(LogError::Truncated)
    }
}

/// Decodes standard base64 with padding, as the runtime writes `Program data:` fields.
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let (quads, []) = text.as_bytes().as_chunks::<4>() else {
        return None;
    };
    let mut bytes = Vec::with_capacity(quads.len() * 3);
    for (index, quad) in quads.iter().enumerate() {
        let padding = quad.iter().rev().take_while(|&&byte| byte == b'=').count();
        if padding > 2 || (padding > 0 && index + 1 != quads.len()) {
            return None;
        }
        let mut value = 0u32;
        for &byte in &quad[..4 - padding] {
            value = value << 6 | u32::from(sextet(byte)?);
        }
        value <<= 6 * padding;
        bytes.extend_from_slice(&value.to_be_bytes()[1..4 - padding]);
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

    const BALLISTA: &str = "BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD";
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

    #[test]
    fn base64_decodes_as_the_runtime_encodes() {
        let cases: [(&str, &[u8]); 6] = [
            ("", b""),
            ("QQ==", b"A"),
            ("QUI=", b"AB"),
            ("QUJD", b"ABC"),
            ("T1VUUgEAAAAAAAAA", b"OUTR\x01\0\0\0\0\0\0\0"),
            ("+/8=", &[0xfb, 0xff]),
        ];
        for (text, bytes) in cases {
            assert_eq!(decode_base64(text).as_deref(), Some(bytes), "{text}");
        }
        for text in ["Q", "QQ=", "Q===", "QQ==QQ==", "Q=QQ", "QU I", "QUJD\n"] {
            assert_eq!(decode_base64(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_run_event_is_47_bytes_starting_with_bev1() {
        let template = Pubkey::new_from_array([7; 32]);
        let event = event_bytes(template);
        assert_eq!(
            decode_run_event(&event),
            Some(RunEvent {
                version: 1,
                iterations: 2,
                expanded: 3,
                executed: 0b101,
                template,
            })
        );
        assert_eq!(decode_run_event(&event[..46]), None, "short");
        assert_eq!(
            decode_run_event(&[event.as_slice(), &[0]].concat()),
            None,
            "long"
        );
        let mut later = event.clone();
        later[3] = b'2';
        assert_eq!(decode_run_event(&later), None, "another event version");
        assert_eq!(decode_run_event(b"PAID"), None);
    }

    /// The lines of a nested run and of a program it calls sit between the outer run's own, and
    /// another program can log bytes that read as a run event.
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
        let attributed: Vec<(Pubkey, usize)> = lines
            .iter()
            .map(|line| (line.program, line.height))
            .collect();
        assert_eq!(
            attributed,
            [(key(JUPITER), 3), (key(BALLISTA), 2), (key(BALLISTA), 1)]
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

    #[test]
    fn logs_that_do_not_nest_are_refused() {
        let invoke = |height: usize| format!("Program {BALLISTA} invoke [{height}]");
        let success = format!("Program {BALLISTA} success");
        let cases: [(Vec<String>, LogError); 7] = [
            (vec!["Program data: QQ==".into()], LogError::Malformed(0)),
            (vec![invoke(2)], LogError::Malformed(0)),
            (vec![invoke(1), invoke(3)], LogError::Malformed(1)),
            (
                vec![invoke(1), format!("Program {JUPITER} success")],
                LogError::Malformed(1),
            ),
            (vec![success.clone()], LogError::Malformed(0)),
            (
                vec![invoke(1), "Program data: QQ".into()],
                LogError::Malformed(1),
            ),
            (vec![invoke(1), "Log truncated".into()], LogError::Truncated),
        ];
        for (logs, error) in cases {
            assert_eq!(program_data(&logs), Err(error), "{logs:?}");
        }
        assert_eq!(
            program_data(&[invoke(1)]),
            Err(LogError::Truncated),
            "never closed"
        );
        let failed = [
            invoke(1),
            format!("Program {BALLISTA} failed: custom program error: 0x1"),
        ];
        assert_eq!(program_data(&failed), Ok(Vec::new()));
    }

    #[test]
    fn a_ballista_line_is_an_event_an_emit_or_neither() {
        let ballista = key(BALLISTA);
        let line = |fields: &[&[u8]]| ProgramDataLine {
            program: ballista,
            height: 1,
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
