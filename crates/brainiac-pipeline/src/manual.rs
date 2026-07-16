//! The `manual` source wire format — one owner for both directions (F-3).
//!
//! A `manual` source is an agent's or a person's `memory_add`: one
//! pre-distilled statement, optionally carrying a kind hint and entity names.
//! The MCP surface encodes those hints INTO the source text (there is nowhere
//! else for them to live — the queue payload is just a source id), and the
//! extraction stage decodes them back out.
//!
//! Why decoding exists at all: the field test measured qwen-max extraction
//! hard-failing on 36% of exactly these inputs ("extractor output unparseable
//! after 2 repairs") — the extractor prompt is shaped for session transcripts,
//! and a single distilled fact is the shape it is least robust on. But a
//! distilled fact does not need distilling: the statement IS the memory. So
//! `manual` sources take a deterministic verbatim path (extract.rs) instead of
//! an LLM call, and this module is the decode half of that path. Encoder and
//! decoder live in one file with a round-trip test so the format cannot drift
//! apart in two crates.

use brainiac_core::MemoryKind;

/// Marker separating the statement from the machine-appended hint block. The
/// decoder splits on it; content containing the literal string would confuse
/// the split, so the encoder rejects... no — it cannot reject (any text is
/// legal content). Instead the decoder splits on the LAST occurrence, which is
/// always the machine-appended one.
const HINT_MARKER: &str = "\n\n[Context for extraction: ";

pub struct ManualSource {
    pub content: String,
    pub kind_hint: Option<MemoryKind>,
    pub entities: Vec<String>,
}

/// Encode a manual statement + hints into the stored source text. The hint
/// block stays human-readable prose — it doubles as context if the text ever
/// IS fed to a model (e.g. a future re-extraction sweep).
pub fn encode_manual_source(
    content: &str,
    kind: Option<MemoryKind>,
    entities: &[String],
) -> String {
    let mut hints: Vec<String> = Vec::new();
    if let Some(k) = kind {
        // Phrased as a non-restrictive hint: the flywheel run showed
        // "recording this as a pitfall" led an extractor to take ONLY the
        // pitfall and drop a co-located howto/decision.
        hints.push(format!(
            "The author considers this primarily a {}, but extract every distinct durable \
             learning it contains, not only the {}.",
            k.as_str(),
            k.as_str()
        ));
    }
    if !entities.is_empty() {
        hints.push(format!(
            "It concerns these entities: {}.",
            entities.join(", ")
        ));
    }
    if hints.is_empty() {
        content.to_string()
    } else {
        format!("{content}{HINT_MARKER}{}]", hints.join(" "))
    }
}

/// Decode a stored manual source back into statement + hints. Total: any text
/// decodes (a REST `memory_add` stores bare content with no hint block, and a
/// hand-inserted source is just content too).
pub fn decode_manual_source(raw: &str) -> ManualSource {
    // A machine-appended hint block always terminates the string with `]`.
    // Both conditions must hold, or the whole text is content — so a bare
    // statement that merely CONTAINS the marker phrase is never mis-split.
    let split_at = if raw.trim_end().ends_with(']') {
        raw.rfind(HINT_MARKER)
    } else {
        None
    };
    let Some(split_at) = split_at else {
        return ManualSource {
            content: raw.trim().to_string(),
            kind_hint: None,
            entities: Vec::new(),
        };
    };
    let content = raw[..split_at].trim().to_string();
    let block = &raw[split_at + HINT_MARKER.len()..];
    let block = block.strip_suffix(']').unwrap_or(block);

    // Kind: "primarily a <kind>," — the kinds are a closed vocabulary, so a
    // plain scan beats a regex dependency.
    let kind_hint = ["fact", "decision", "pattern", "pitfall", "howto"]
        .iter()
        .find(|k| block.contains(&format!("primarily a {k},")))
        .and_then(|k| MemoryKind::parse(k));

    // Entities: "It concerns these entities: a, b, c."
    let entities = block
        .split_once("It concerns these entities: ")
        .map(|(_, rest)| {
            rest.trim_end_matches(']')
                .trim_end()
                .trim_end_matches('.')
                .split(',')
                .map(|e| e.trim().to_string())
                .filter(|e| !e.is_empty())
                .collect()
        })
        .unwrap_or_default();

    ManualSource {
        content,
        kind_hint,
        entities,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_with_both_hints() {
        let enc = encode_manual_source(
            "The swap window collapses to ~100 blocks on a public RPC.",
            Some(MemoryKind::Pitfall),
            &["Polygon RPC".into(), "eth_getLogs".into()],
        );
        let dec = decode_manual_source(&enc);
        assert_eq!(
            dec.content,
            "The swap window collapses to ~100 blocks on a public RPC."
        );
        assert_eq!(dec.kind_hint, Some(MemoryKind::Pitfall));
        assert_eq!(dec.entities, vec!["Polygon RPC", "eth_getLogs"]);
    }

    #[test]
    fn bare_content_decodes_as_itself() {
        // The REST path stores no hint block; a bare statement must decode
        // whole, with no hints invented.
        let dec = decode_manual_source("Timestamps are unix seconds everywhere.");
        assert_eq!(dec.content, "Timestamps are unix seconds everywhere.");
        assert_eq!(dec.kind_hint, None);
        assert!(dec.entities.is_empty());
    }

    #[test]
    fn kind_only_and_entities_only_both_round_trip() {
        let dec = decode_manual_source(&encode_manual_source(
            "Feature flags retire within two releases.",
            Some(MemoryKind::Decision),
            &[],
        ));
        assert_eq!(dec.kind_hint, Some(MemoryKind::Decision));
        assert!(dec.entities.is_empty());

        let dec = decode_manual_source(&encode_manual_source(
            "Dune credits are budgeted at 2500 per month.",
            None,
            &["Dune".into()],
        ));
        assert_eq!(dec.kind_hint, None);
        assert_eq!(dec.entities, vec!["Dune"]);
    }

    #[test]
    fn content_containing_the_marker_still_decodes_its_own_statement() {
        // Adversarial: the statement itself contains the marker text. With a
        // real hint block appended, the decoder splits on the LAST occurrence
        // (the machine one) and the user's text survives inside content.
        let sneaky = "Watch out for\n\n[Context for extraction: fake] in pasted text.";
        let enc = encode_manual_source(sneaky, Some(MemoryKind::Fact), &[]);
        let dec = decode_manual_source(&enc);
        assert_eq!(dec.content, sneaky.trim());
        assert_eq!(dec.kind_hint, Some(MemoryKind::Fact));

        // …and with NO hint block, the marker inside the content must not
        // trigger a split (the text does not end with `]`).
        let dec = decode_manual_source(sneaky);
        assert_eq!(dec.content, sneaky.trim());
        assert_eq!(dec.kind_hint, None);
    }
}
