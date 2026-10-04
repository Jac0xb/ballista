//! The TypeScript and Rust template compilers, side by side: each document the compiler fuzzer
//! writes (`clients/js/src/compiler-differential.test.ts`) is built with the Rust builders and
//! compiled with `ballista_sdk::template`. Where TypeScript compiles it, Rust must give the same
//! bytes; where TypeScript refuses it, Rust must refuse it too.
//!
//! The committed fixture is a small slice (`COMPILER_DIFF_SEEDS=4 COMPILER_DIFF_GROUP_DOCUMENTS=10`).
//! For a full run, write a corpus and point at it:
//!
//!     COMPILER_DIFF_OUT=/tmp/corpus.json COMPILER_DIFF_SEEDS=2000 \
//!       pnpm --dir clients/js exec vitest run compiler-differential -t differential
//!     COMPILER_DIFF_CORPUS=/tmp/corpus.json cargo test -p ballista-sdk --test template_differential
//!
//! A document the Rust builders cannot express (an unknown key, a number out of a builder's
//! range) is counted, not compared: TypeScript must refuse every such document.

use std::panic::{catch_unwind, AssertUnwindSafe};

use ballista_sdk::template::prelude::Pubkey;
use ballista_sdk::template::{
    account, data, expr, step, Account, AccountField, AccountRef, Batch, DataPart, Encoding, Expr,
    GroupFilter, Offset, ReadType, Step, Template, Type,
};

const COMMITTED: &str = include_str!("fixtures/compiler-differential.json");

// ---------------------------------------------------------------------------------------------
// A small JSON reader that keeps object keys in order (declaration order matters)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

struct Reader<'a> {
    text: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn space(&mut self) {
        while self.at < self.text.len() && self.text[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }

    fn expect(&mut self, byte: u8) {
        self.space();
        assert_eq!(
            self.text[self.at], byte,
            "JSON: expected {} at {}",
            byte as char, self.at
        );
        self.at += 1;
    }

    fn value(&mut self) -> Json {
        self.space();
        match self.text[self.at] {
            b'{' => {
                self.at += 1;
                let mut fields = Vec::new();
                self.space();
                if self.text[self.at] == b'}' {
                    self.at += 1;
                    return Json::Object(fields);
                }
                loop {
                    self.space();
                    let key = self.string();
                    self.expect(b':');
                    fields.push((key, self.value()));
                    self.space();
                    self.at += 1;
                    if self.text[self.at - 1] == b'}' {
                        return Json::Object(fields);
                    }
                }
            }
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                self.space();
                if self.text[self.at] == b']' {
                    self.at += 1;
                    return Json::Array(items);
                }
                loop {
                    items.push(self.value());
                    self.space();
                    self.at += 1;
                    if self.text[self.at - 1] == b']' {
                        return Json::Array(items);
                    }
                }
            }
            b'"' => Json::String(self.string()),
            b't' => {
                self.at += 4;
                Json::Bool(true)
            }
            b'f' => {
                self.at += 5;
                Json::Bool(false)
            }
            b'n' => {
                self.at += 4;
                Json::Null
            }
            _ => {
                let start = self.at;
                while self.at < self.text.len() && b"+-.eE0123456789".contains(&self.text[self.at])
                {
                    self.at += 1;
                }
                let digits = std::str::from_utf8(&self.text[start..self.at]).unwrap();
                Json::Number(
                    digits
                        .parse()
                        .unwrap_or_else(|_| panic!("JSON: number {digits}")),
                )
            }
        }
    }

    fn hex4(&mut self) -> u32 {
        let digits = std::str::from_utf8(&self.text[self.at..self.at + 4]).unwrap();
        self.at += 4;
        u32::from_str_radix(digits, 16).unwrap()
    }

    fn string(&mut self) -> String {
        self.expect(b'"');
        let mut out = Vec::new();
        loop {
            let byte = self.text[self.at];
            self.at += 1;
            match byte {
                b'"' => return String::from_utf8(out).unwrap(),
                b'\\' => {
                    let escape = self.text[self.at];
                    self.at += 1;
                    let character = match escape {
                        b'n' => '\n',
                        b't' => '\t',
                        b'r' => '\r',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'u' => {
                            let high = self.hex4();
                            let code = if (0xd800..0xdc00).contains(&high)
                                && self.text[self.at] == b'\\'
                            {
                                self.at += 2;
                                0x10000 + ((high - 0xd800) << 10) + (self.hex4() - 0xdc00)
                            } else {
                                high
                            };
                            char::from_u32(code).unwrap_or('\u{fffd}')
                        }
                        other => other as char,
                    };
                    let mut buffer = [0; 4];
                    out.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
                }
                other => out.push(other),
            }
        }
    }
}

fn parse(text: &str) -> Json {
    Reader {
        text: text.as_bytes(),
        at: 0,
    }
    .value()
}

/// Why a document cannot be built with the Rust builders.
type Built<T> = Result<T, String>;

impl Json {
    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    fn field(&self, key: &str) -> Built<&Json> {
        self.get(key).ok_or_else(|| format!("no `{key}`"))
    }

    fn fields(&self) -> Built<&[(String, Json)]> {
        match self {
            Json::Object(fields) => Ok(fields),
            other => Err(format!("not an object: {other:?}")),
        }
    }

    /// An object with only these keys, as TypeScript's strict schemas require.
    fn only(&self, keys: &[&str]) -> Built<&Json> {
        for (name, _) in self.fields()? {
            if !keys.contains(&name.as_str()) {
                return Err(format!("unknown key `{name}`"));
            }
        }
        Ok(self)
    }

    fn text(&self) -> Built<&str> {
        match self {
            Json::String(text) => Ok(text),
            other => Err(format!("not a string: {other:?}")),
        }
    }

    fn items(&self) -> Built<&[Json]> {
        match self {
            Json::Array(items) => Ok(items),
            other => Err(format!("not an array: {other:?}")),
        }
    }

    fn flag(&self) -> Built<bool> {
        match self {
            Json::Bool(value) => Ok(*value),
            other => Err(format!("not a bool: {other:?}")),
        }
    }

    fn integer<T: TryFrom<i64>>(&self) -> Built<T> {
        match self {
            Json::Number(value) if value.fract() == 0.0 && value.abs() < 9e15 => {
                T::try_from(*value as i64).map_err(|_| format!("{value} out of range"))
            }
            other => Err(format!("not an integer in range: {other:?}")),
        }
    }

    /// A `{ "$bigint": "-12" }`: its sign (true for negative) and magnitude.
    fn big(&self) -> Built<(bool, u128)> {
        let digits = self.only(&["$bigint"])?.field("$bigint")?.text()?;
        let (negative, magnitude) = digits
            .strip_prefix('-')
            .map_or((false, digits), |rest| (true, rest));
        Ok((
            negative,
            magnitude.parse().map_err(|_| format!("bigint {digits}"))?,
        ))
    }

    fn bytes(&self) -> Built<Vec<u8>> {
        let digits = self.only(&["$bytes"])?.field("$bytes")?.text()?;
        (0..digits.len())
            .step_by(2)
            .map(|at| {
                u8::from_str_radix(&digits[at..at + 2], 16).map_err(|error| error.to_string())
            })
            .collect()
    }

    fn key32(&self) -> Built<Pubkey> {
        let bytes: [u8; 32] = self
            .bytes()?
            .try_into()
            .map_err(|_| "not 32 bytes".to_string())?;
        Ok(Pubkey::from(bytes))
    }

    fn optional_flag(&self, key: &str) -> Built<bool> {
        self.get(key).map_or(Ok(false), Json::flag)
    }
}

// ---------------------------------------------------------------------------------------------
// Documents to builders
// ---------------------------------------------------------------------------------------------

fn value_type(node: &Json) -> Built<Type> {
    let kind = node.field("type")?.text()?;
    if kind != "bytes" {
        node.only(&["type"])?;
    }
    Ok(match kind {
        "bool" => Type::Bool,
        "u64" => Type::U64,
        "i64" => Type::I64,
        "u128" => Type::U128,
        "pubkey" => Type::Pubkey,
        "bytes" => Type::Bytes(
            node.only(&["type", "maxLength"])?
                .field("maxLength")?
                .integer()?,
        ),
        other => return Err(format!("input type {other}")),
    })
}

fn field_type(name: &str) -> Built<Type> {
    Ok(match name {
        "bool" => Type::Bool,
        "u64" => Type::U64,
        "i64" => Type::I64,
        "u128" => Type::U128,
        "pubkey" => Type::Pubkey,
        other => return Err(format!("registry field type {other}")),
    })
}

fn read_type(node: &Json) -> Built<ReadType> {
    Ok(match node.text()? {
        "bool" => ReadType::Bool,
        "u8" => ReadType::U8,
        "u16" => ReadType::U16,
        "u32" => ReadType::U32,
        "i32" => ReadType::I32,
        "u64" => ReadType::U64,
        "i64" => ReadType::I64,
        "u128" => ReadType::U128,
        "pubkey" => ReadType::Pubkey,
        other => return Err(format!("read type {other}")),
    })
}

fn reference(node: &Json) -> Built<AccountRef> {
    let name = node.only(&["kind", "name"])?.field("name")?.text()?;
    match node.field("kind")?.text()? {
        "account" => Ok(account::fixed(name)),
        "iterationAccount" => Ok(account::iteration(name)),
        other => Err(format!("account reference {other}")),
    }
}

fn literal(node: &Json) -> Built<Expr> {
    let node = node.only(&["type", "value"])?;
    let value = node.field("value")?;
    Ok(match node.field("type")?.text()? {
        "bool" => expr::bool(value.flag()?),
        "u64" => match value.big()? {
            (false, magnitude) => expr::u64(u64::try_from(magnitude).map_err(|_| "u64 range")?),
            _ => return Err("negative u64".into()),
        },
        "i64" => {
            let (negative, magnitude) = value.big()?;
            let signed = i128::try_from(magnitude).map_err(|_| "i64 range")?;
            expr::i64(
                i64::try_from(if negative { -signed } else { signed }).map_err(|_| "i64 range")?,
            )
        }
        "u128" => match value.big()? {
            (false, magnitude) => expr::u128(magnitude),
            _ => return Err("negative u128".into()),
        },
        "pubkey" => expr::pubkey(value.key32()?),
        "bytes" => expr::bytes(value.bytes()?),
        other => return Err(format!("literal type {other}")),
    })
}

fn boxed(node: &Json, key: &str) -> Built<Expr> {
    expression(node.field(key)?)
}

fn group_filter(node: &Json) -> Built<GroupFilter> {
    let node = node.only(&["programs", "minDataLength", "match", "exceptKeys"])?;
    let mut filter = GroupFilter::new();
    for program in node.field("programs")?.items()? {
        filter = filter.program(program.key32()?);
    }
    if let Some(length) = node.get("minDataLength") {
        filter = filter.min_data_length(length.integer()?);
    }
    for segment in node.field("match")?.items()? {
        let segment = segment.only(&["offset", "equals"])?;
        filter = filter.equals(
            segment.field("offset")?.integer()?,
            boxed(segment, "equals")?,
        );
    }
    if let Some(keys) = node.get("exceptKeys") {
        for key in keys.items()? {
            filter = filter.except_key(expression(key)?);
        }
    }
    Ok(filter)
}

fn expression(node: &Json) -> Built<Expr> {
    let kind = node.field("kind")?.text()?;
    let shape: &[&str] = match kind {
        "input" | "rowInput" | "variable" => &["kind", "name"],
        "literal" => &["kind", "value"],
        "accountField" => &["kind", "account", "field"],
        "accountData" => &["kind", "account", "offset", "type"],
        "returnData" => &["kind", "offset", "type"],
        "clock" => &["kind", "field"],
        "loopIndex" => &["kind"],
        "pda" => &["kind", "program", "seeds", "bump"],
        "binary" => &["kind", "op", "left", "right"],
        "multiplyDivide" => &["kind", "left", "right", "divisor", "rounding"],
        "powerOfTen" => &["kind", "exponent"],
        "not" | "bytesLength" => &["kind", "value"],
        "select" => &["kind", "condition", "ifTrue", "ifFalse"],
        "cast" => &["kind", "to", "value"],
        "instructionCount" | "currentInstructionIndex" => &["kind", "sysvar"],
        "instruction" => &["kind", "sysvar", "index", "field"],
        "instructionAccount" => &["kind", "sysvar", "index", "position", "field"],
        "instructionData" => &["kind", "sysvar", "index", "offset", "type"],
        "instructionDataBytes" => &["kind", "sysvar", "index", "offset", "length"],
        "accountDataBytes" => &["kind", "account", "offset", "length"],
        "registry" => &["kind", "account", "field"],
        "groupLength" => &["kind", "group"],
        "groupAny" | "groupCount" => &["kind", "group", "filter"],
        other => return Err(format!("expression kind {other}")),
    };
    node.only(shape)?;
    let name = || node.field("name").and_then(Json::text);
    Ok(match kind {
        "input" => expr::input(name()?),
        "rowInput" => expr::row_input(name()?),
        "variable" => expr::var(name()?),
        "literal" => literal(node.field("value")?)?,
        "accountField" => expr::account_field(
            reference(node.field("account")?)?,
            match node.field("field")?.text()? {
                "key" => AccountField::Key,
                "owner" => AccountField::Owner,
                "lamports" => AccountField::Lamports,
                "dataLength" => AccountField::DataLength,
                "isEmpty" => AccountField::IsEmpty,
                other => return Err(format!("account field {other}")),
            },
        ),
        "accountData" => {
            let offset = node.field("offset")?;
            let offset = match offset {
                Json::Number(_) => Offset::Static(offset.integer()?),
                _ => Offset::Dynamic(expression(offset)?),
            };
            expr::account_data(
                reference(node.field("account")?)?,
                offset,
                read_type(node.field("type")?)?,
            )
        }
        "returnData" => expr::return_data_at(
            read_type(node.field("type")?)?,
            node.field("offset")?.integer()?,
        ),
        "clock" => match node.field("field")?.text()? {
            "slot" => expr::clock_slot(),
            "unixTimestamp" => expr::clock_unix_timestamp(),
            other => return Err(format!("clock field {other}")),
        },
        "loopIndex" => expr::loop_index(),
        "pda" => {
            let program = reference(node.field("program")?)?;
            let seeds = node
                .field("seeds")?
                .items()?
                .iter()
                .map(expression)
                .collect::<Built<Vec<_>>>()?;
            match node.get("bump") {
                Some(bump) => expr::pda_with_bump(program, seeds, expression(bump)?),
                None => expr::pda(program, seeds),
            }
        }
        "binary" => {
            let (left, right) = (boxed(node, "left")?, boxed(node, "right")?);
            match node.field("op")?.text()? {
                "add" => expr::add(left, right),
                "subtract" => expr::subtract(left, right),
                "multiply" => expr::multiply(left, right),
                "divide" => expr::divide(left, right),
                "min" => expr::min(left, right),
                "max" => expr::max(left, right),
                "equal" => expr::equal(left, right),
                "notEqual" => expr::not_equal(left, right),
                "lessThan" => expr::less_than(left, right),
                "lessThanOrEqual" => expr::less_than_or_equal(left, right),
                "greaterThan" => expr::greater_than(left, right),
                "greaterThanOrEqual" => expr::greater_than_or_equal(left, right),
                "and" => expr::and(left, right),
                "or" => expr::or(left, right),
                "remainder" => expr::remainder(left, right),
                "shiftLeft" => expr::shift_left(left, right),
                "shiftRight" => expr::shift_right(left, right),
                "bitAnd" => expr::bit_and(left, right),
                "bitOr" => expr::bit_or(left, right),
                "bitXor" => expr::bit_xor(left, right),
                other => return Err(format!("binary op {other}")),
            }
        }
        "multiplyDivide" => {
            let (left, right, divisor) = (
                boxed(node, "left")?,
                boxed(node, "right")?,
                boxed(node, "divisor")?,
            );
            match node.field("rounding")?.text()? {
                "down" => expr::multiply_divide(left, right, divisor),
                "up" => expr::multiply_divide_up(left, right, divisor),
                other => return Err(format!("rounding {other}")),
            }
        }
        "powerOfTen" => expr::power_of_ten(boxed(node, "exponent")?),
        "not" => expr::not(boxed(node, "value")?),
        "bytesLength" => expr::bytes_length(boxed(node, "value")?),
        "select" => expr::select(
            boxed(node, "condition")?,
            boxed(node, "ifTrue")?,
            boxed(node, "ifFalse")?,
        ),
        "cast" => expr::cast(
            match node.field("to")?.text()? {
                "u64" => Type::U64,
                "i64" => Type::I64,
                "u128" => Type::U128,
                other => return Err(format!("cast to {other}")),
            },
            boxed(node, "value")?,
        ),
        "instructionCount" => expr::instruction_count(reference(node.field("sysvar")?)?),
        "currentInstructionIndex" => {
            expr::current_instruction_index(reference(node.field("sysvar")?)?)
        }
        "instruction" => {
            let (sysvar, index) = (reference(node.field("sysvar")?)?, boxed(node, "index")?);
            match node.field("field")?.text()? {
                "program" => expr::instruction_program(sysvar, index),
                "accountCount" => expr::instruction_account_count(sysvar, index),
                "dataLength" => expr::instruction_data_length(sysvar, index),
                other => return Err(format!("instruction field {other}")),
            }
        }
        "instructionAccount" => {
            let sysvar = reference(node.field("sysvar")?)?;
            let (index, position) = (boxed(node, "index")?, boxed(node, "position")?);
            match node.field("field")?.text()? {
                "key" => expr::instruction_account(sysvar, index, position),
                "flags" => expr::instruction_account_flags(sysvar, index, position),
                other => return Err(format!("instruction account field {other}")),
            }
        }
        "instructionData" => expr::instruction_data(
            reference(node.field("sysvar")?)?,
            boxed(node, "index")?,
            boxed(node, "offset")?,
            read_type(node.field("type")?)?,
        ),
        "instructionDataBytes" => expr::instruction_data_bytes(
            reference(node.field("sysvar")?)?,
            boxed(node, "index")?,
            boxed(node, "offset")?,
            node.field("length")?.integer::<u32>()? as usize,
        ),
        "accountDataBytes" => expr::account_data_bytes(
            reference(node.field("account")?)?,
            boxed(node, "offset")?,
            node.field("length")?.integer::<u32>()? as usize,
        ),
        "registry" => expr::registry(node.field("account")?.text()?, node.field("field")?.text()?),
        "groupLength" => expr::group_length(node.field("group")?.text()?),
        "groupAny" => expr::group_any(
            node.field("group")?.text()?,
            group_filter(node.field("filter")?)?,
        ),
        "groupCount" => expr::group_count(
            node.field("group")?.text()?,
            group_filter(node.field("filter")?)?,
        ),
        _ => unreachable!(),
    })
}

fn part(node: &Json) -> Built<DataPart> {
    match node.field("kind")?.text()? {
        "literal" => Ok(data::literal(
            node.only(&["kind", "bytes"])?.field("bytes")?.bytes()?,
        )),
        "encoded" => {
            let node = node.only(&["kind", "encoding", "value"])?;
            let encoding = match node.field("encoding")?.text()? {
                "u8" => Encoding::U8,
                "u16" => Encoding::U16,
                "u32" => Encoding::U32,
                "u64" => Encoding::U64,
                "i64" => Encoding::I64,
                "u128" => Encoding::U128,
                "pubkey" => Encoding::Pubkey,
                "bool" => Encoding::Bool,
                "bytes" => Encoding::Bytes,
                other => return Err(format!("encoding {other}")),
            };
            Ok(data::encode(encoding, boxed(node, "value")?))
        }
        other => Err(format!("data part {other}")),
    }
}

fn parts(node: &Json, key: &str) -> Built<Vec<DataPart>> {
    node.field(key)?.items()?.iter().map(part).collect()
}

fn steps(node: &Json) -> Built<Vec<Step>> {
    node.items()?.iter().map(one_step).collect()
}

fn one_step(node: &Json) -> Built<Step> {
    let kind = node.field("kind")?.text()?;
    let shape: &[&str] = match kind {
        "require" => &["kind", "condition", "label"],
        "let" | "assign" => &["kind", "name", "value", "label"],
        "invoke" => &[
            "kind",
            "program",
            "accounts",
            "data",
            "when",
            "accountGroup",
            "programAddress",
            "label",
        ],
        "emit" | "setReturnData" => &["kind", "parts", "label"],
        "setRegistry" => &["kind", "account", "field", "value", "label"],
        "forEach" => &["kind", "steps", "carry", "label"],
        "repeat" => &["kind", "count", "max", "steps", "carry", "label"],
        other => return Err(format!("step kind {other}")),
    };
    node.only(shape)?;
    let label = node.get("label").map(Json::text).transpose()?;
    let name = || node.field("name").and_then(Json::text);
    let carry = |mut body: ballista_sdk::template::Loop| -> Built<ballista_sdk::template::Loop> {
        if let Some(carried) = node.get("carry") {
            for name in carried.items()? {
                body = body.carry(name.text()?);
            }
        }
        body = body.steps(steps(node.field("steps")?)?);
        Ok(match label {
            Some(label) => body.label(label),
            None => body,
        })
    };
    let built: Step = match kind {
        "require" => step::require(boxed(node, "condition")?),
        "let" => step::let_(name()?, boxed(node, "value")?),
        "assign" => step::assign(name()?, boxed(node, "value")?),
        "emit" => step::emit(parts(node, "parts")?),
        "setReturnData" => step::set_return_data(parts(node, "parts")?),
        "setRegistry" => step::set_registry(
            node.field("account")?.text()?,
            node.field("field")?.text()?,
            boxed(node, "value")?,
        ),
        "invoke" => {
            let mut invoke = step::invoke(reference(node.field("program")?)?);
            for entry in node.field("accounts")?.items()? {
                let entry = entry.only(&["account", "signer", "writable"])?;
                invoke = invoke.account(
                    reference(entry.field("account")?)?,
                    entry.optional_flag("signer")?,
                    entry.optional_flag("writable")?,
                );
            }
            invoke = invoke.data_parts(parts(node, "data")?);
            if let Some(when) = node.get("when") {
                invoke = invoke.when(expression(when)?);
            }
            if let Some(group) = node.get("accountGroup") {
                invoke = invoke.account_group(group.text()?);
            }
            if let Some(address) = node.get("programAddress") {
                invoke = invoke.program_address(address.key32()?);
            }
            return Ok(match label {
                Some(label) => invoke.label(label).into(),
                None => invoke.into(),
            });
        }
        "forEach" => return carry(step::for_each()).map(Step::from),
        "repeat" => {
            return carry(step::repeat(
                boxed(node, "count")?,
                node.field("max")?.integer()?,
            ))
            .map(Step::from)
        }
        _ => unreachable!(),
    };
    Ok(match label {
        Some(label) => built.label(label),
        None => built,
    })
}

fn constraint(node: &Json) -> Built<Account> {
    let node = node.only(&[
        "signer",
        "writable",
        "executable",
        "address",
        "owner",
        "minDataLength",
        "unsafeUnpinned",
        "registry",
    ])?;
    let writable = node.optional_flag("writable")?;
    let mut built = match node.get("registry") {
        Some(registry) => {
            let registry = registry.only(&["name", "key", "payer"])?;
            if !writable {
                // `account::registry` is always writable.
                return Err("a registry account that is not writable".into());
            }
            let mut built = account::registry(
                registry.field("name")?.text()?,
                registry.field("payer")?.text()?,
            );
            if let Some(key) = registry.get("key") {
                built = built.key(expression(key)?);
            }
            built
        }
        None => Account::new(),
    };
    if node.optional_flag("signer")? {
        built = built.signer();
    }
    if writable {
        built = built.writable();
    }
    if node.optional_flag("executable")? {
        built = built.executable();
    }
    if let Some(address) = node.get("address") {
        built = built.address(address.key32()?);
    }
    if let Some(owner) = node.get("owner") {
        built = built.owner(owner.key32()?);
    }
    if let Some(length) = node.get("minDataLength") {
        built = built.min_data_length(length.integer()?);
    }
    if node.optional_flag("unsafeUnpinned")? {
        built = built.unsafe_unpinned();
    }
    Ok(built)
}

fn template(node: &Json) -> Built<Template> {
    let node = node.only(&[
        "version",
        "inputs",
        "registries",
        "accounts",
        "batch",
        "emitEvent",
        "accountGroups",
        "steps",
    ])?;
    if let Some(version) = node.get("version") {
        if version.integer::<i64>()? != 1 {
            return Err("version".into());
        }
    }
    let mut built = Template::new();
    if let Some(inputs) = node.get("inputs") {
        for (name, definition) in inputs.fields()? {
            built = built.input(name, value_type(definition)?);
        }
    }
    if let Some(registries) = node.get("registries") {
        for (name, layout) in registries.fields()? {
            let fields = layout
                .fields()?
                .iter()
                .map(|(field, ty)| Ok((field.clone(), field_type(ty.text()?)?)))
                .collect::<Built<Vec<_>>>()?;
            built = built.registry(name, fields);
        }
    }
    for (name, definition) in node.field("accounts")?.fields()? {
        built = built.account(name, constraint(definition)?);
    }
    if let Some(batch) = node.get("batch") {
        let batch = batch.only(&["maxIterations", "minIterations", "row", "rowInputs"])?;
        let mut rows = Batch::new(batch.field("maxIterations")?.integer()?);
        if let Some(minimum) = batch.get("minIterations") {
            rows = rows.min_iterations(minimum.integer()?);
        }
        for (name, definition) in batch.field("row")?.fields()? {
            rows = rows.account(name, constraint(definition)?);
        }
        if let Some(inputs) = batch.get("rowInputs") {
            for (name, definition) in inputs.fields()? {
                rows = rows.input(name, value_type(definition)?);
            }
        }
        built = built.batch(rows);
    }
    if node.optional_flag("emitEvent")? {
        built = built.emit_event();
    }
    if let Some(groups) = node.get("accountGroups") {
        for group in groups.items()? {
            built = built.account_group(group.text()?);
        }
    }
    Ok(built.steps(steps(node.field("steps")?)?))
}

// ---------------------------------------------------------------------------------------------
// The comparison
// ---------------------------------------------------------------------------------------------

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Default, Debug)]
struct Tally {
    documents: usize,
    same_bytes: usize,
    both_refuse: usize,
    same_message: usize,
    unbuildable: usize,
    /// Rust compiles it and TypeScript refuses it for its open finding `proto-name`.
    known: usize,
    groups_compared: usize,
    hazards_compared: usize,
    mismatches: Vec<String>,
}

fn compare(corpus: &str) -> Tally {
    let mut tally = Tally::default();
    for entry in parse(corpus).items().unwrap() {
        tally.documents += 1;
        let name = entry.field("name").unwrap().text().unwrap().to_string();
        let expected = entry
            .get("bytes")
            .map(|bytes| bytes.text().unwrap().to_string());
        let message = entry
            .get("error")
            .map(|error| error.text().unwrap().to_string());
        let built = match template(entry.field("document").unwrap()) {
            Ok(built) => built,
            Err(why) => {
                tally.unbuildable += 1;
                if expected.is_some() {
                    tally.mismatches.push(format!("{name}: TypeScript compiles it, the Rust builders cannot express it: {why}"));
                }
                continue;
            }
        };
        let outcome = match catch_unwind(AssertUnwindSafe(|| built.compile())) {
            Ok(outcome) => outcome,
            Err(_) => {
                tally
                    .mismatches
                    .push(format!("{name}: the Rust compiler panicked"));
                continue;
            }
        };
        if entry
            .get("groups")
            .is_some_and(|groups| matches!(groups, Json::Bool(true)))
        {
            tally.groups_compared += 1;
        }
        if entry
            .get("hazard")
            .is_some_and(|hazard| matches!(hazard, Json::Bool(true)))
        {
            tally.hazards_compared += 1;
        }
        match (outcome, expected) {
            (Ok(compiled), Some(expected)) => {
                let actual = hex(&compiled.bytes);
                if actual == expected {
                    tally.same_bytes += 1;
                } else {
                    let at = actual
                        .bytes()
                        .zip(expected.bytes())
                        .position(|(left, right)| left != right)
                        .unwrap_or(actual.len().min(expected.len()))
                        / 2;
                    tally.mismatches.push(format!(
                        "{name}: bytes differ from byte {at} (Rust {} bytes, TypeScript {})",
                        actual.len() / 2,
                        expected.len() / 2
                    ));
                }
            }
            (Err(error), None) => {
                tally.both_refuse += 1;
                if message.as_deref() == Some(error.message()) {
                    tally.same_message += 1;
                }
            }
            // Open TypeScript finding `proto-name`: a declaration named `__proto__` vanishes.
            (Ok(_), None)
                if message
                    .as_deref()
                    .is_some_and(|message| message.contains("__proto__")) =>
            {
                tally.known += 1
            }
            (Ok(_), None) => tally.mismatches.push(format!(
                "{name}: Rust compiles it, TypeScript refuses it: {}",
                message.unwrap_or_default()
            )),
            (Err(error), Some(_)) => tally.mismatches.push(format!(
                "{name}: TypeScript compiles it, Rust refuses it: {}",
                error.message()
            )),
        }
    }
    tally
}

fn run(corpus: String) -> Tally {
    // Deeply nested documents recurse in both the reader and the compiler.
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || compare(&corpus))
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn the_rust_compiler_gives_the_typescript_bytes_for_fuzzer_documents() {
    let corpus = match std::env::var("COMPILER_DIFF_CORPUS") {
        Ok(path) => {
            std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
        }
        Err(_) => COMMITTED.to_string(),
    };
    let tally = run(corpus);
    eprintln!(
        "{} documents: {} same bytes, {} refused by both ({} with the same message), {} not expressible, \
         {} TypeScript proto-name refusals, {} with groups and {} with a carried alias compared, {} mismatches",
        tally.documents,
        tally.same_bytes,
        tally.both_refuse,
        tally.same_message,
        tally.unbuildable,
        tally.known,
        tally.groups_compared,
        tally.hazards_compared,
        tally.mismatches.len()
    );
    if let Ok(path) = std::env::var("COMPILER_DIFF_REPORT") {
        std::fs::write(path, tally.mismatches.join("\n")).unwrap();
    }
    assert!(tally.same_bytes > 0, "nothing compiled");
    assert!(
        tally.mismatches.is_empty(),
        "{} mismatches:\n{}",
        tally.mismatches.len(),
        tally.mismatches.join("\n")
    );
}
