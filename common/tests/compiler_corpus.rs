//! The verifier accepts everything the TypeScript compiler emits for the compiler fuzzer's corpus.
//!
//! `clients/js/src/compiler-fuzz.test.ts` generates template documents from seeds and writes, for
//! each one, the natural compile, the same compile with register reuse forced, and a copy whose
//! `let` aliases are materialized. Each of those payloads must parse and verify: a rejection means
//! the compiler emitted bytes the chain would refuse to store.
//!
//! The committed corpus (`fixtures/compiler-fuzz-corpus.json`) is small. To check thousands:
//!
//! ```text
//! FUZZ_SEEDS=5000 FUZZ_CORPUS=/tmp/compiler-corpus.json pnpm --dir clients/js exec vitest run src/compiler-fuzz.test.ts
//! COMPILER_FUZZ_CORPUS=/tmp/compiler-corpus.json cargo test -p ballista-common --test compiler_corpus -- --nocapture
//! ```

use ballista_common::template::ProgramView;

const COMMITTED: &str = include_str!("../../fixtures/compiler-fuzz-corpus.json");
const VARIANTS: [&str; 3] = ["natural", "forced", "materialized"];

/// Each payload in the corpus, with the seed and variant it belongs to, in file order. The file is
/// JSON, but `ballista-common` takes no JSON dependency: the corpus writer puts each case on one
/// line, with its `"seed"` and each variant as `"<variant>":{"payload":"<hex>"`, which is all this
/// reads.
fn payloads(corpus: &str) -> Vec<(u64, &'static str, &str)> {
    let mut found = Vec::new();
    for line in corpus.lines().filter(|line| line.contains("\"seed\":")) {
        let seed = line
            .split("\"seed\":")
            .nth(1)
            .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|digits| digits.parse().ok())
            .expect("a case has a numeric seed");
        for variant in VARIANTS {
            let key = format!("\"{variant}\":{{\"payload\":\"");
            if let Some(start) = line.find(&key) {
                let rest = &line[start + key.len()..];
                found.push((
                    seed,
                    variant,
                    &rest[..rest.find('"').expect("closing quote")],
                ));
            }
        }
    }
    found
}

fn decode_hex(value: &str) -> Vec<u8> {
    assert!(value.len() % 2 == 0, "odd-length hex");
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).expect("hex");
            let low = (pair[1] as char).to_digit(16).expect("hex");
            ((high << 4) | low) as u8
        })
        .collect()
}

#[test]
fn every_payload_the_compiler_emitted_verifies() {
    let external = std::env::var("COMPILER_FUZZ_CORPUS").ok().map(|path| {
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {path}: {error}"))
    });
    let corpus = external.as_deref().unwrap_or(COMMITTED);
    let payloads = payloads(corpus);
    assert!(!payloads.is_empty(), "the corpus holds no payloads");
    let mut rejected = Vec::new();
    let mut counts = [0usize; 3];
    for (seed, variant, hex) in &payloads {
        let bytes = decode_hex(hex);
        match ProgramView::parse(&bytes).and_then(|program| program.verify()) {
            Ok(_) => counts[VARIANTS.iter().position(|name| name == variant).unwrap()] += 1,
            Err(error) => rejected.push(format!("seed {seed} {variant}: {error:?}")),
        }
    }
    println!(
        "compiler corpus: {} payloads verified ({} natural, {} forced reuse, {} materialized), {} rejected",
        counts.iter().sum::<usize>(),
        counts[0],
        counts[1],
        counts[2],
        rejected.len()
    );
    assert!(
        rejected.is_empty(),
        "the verifier rejected {} compiled payloads:\n{}",
        rejected.len(),
        rejected.join("\n")
    );
}
