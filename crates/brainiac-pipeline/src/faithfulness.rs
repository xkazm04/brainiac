//! Runtime citation-faithfulness judge (migration 0036).
//!
//! The compose stage's citation firewall is deliberately coarse: it catches an
//! uncited paragraph and an invented id, but not the subtler failure — a
//! sentence that cites a REAL memory while misstating what it says. Until now
//! that check lived only in the offline docs eval; this module runs a sampled
//! version at runtime, on exactly the revisions where it pays: the ones already
//! headed to a human (`needs_review`). The verdict is triage for the reviewer
//! ("read these two paragraphs first"), never a gate — a judge that could block
//! publication would be a second policy engine, and the org already has one.
//!
//! Three self-imposed limits keep it honest and cheap:
//! - **Sampled.** At most [`MAX_JUDGED_PARAGRAPHS`] paragraphs per revision,
//!   spread across the page; `checked` records how many so absence of a flag
//!   on an unchecked paragraph means nothing.
//! - **Best-effort.** Any failure — model, parse, storage — is a warning. A
//!   revision must never fail to land because its critic crashed.
//! - **Advisory.** Verdicts are stored on the revision and shown to the
//!   reviewer; they change no policy decision. (The judge shares the compose
//!   provider, so a systematically wrong model grades its own homework —
//!   a reason it must inform a human rather than replace one.)

use anyhow::{Context, Result};
use brainiac_gateway::{ChatProvider, ChatRequest};
use brainiac_store::Store;
use sqlx::Row;
use uuid::Uuid;

/// Cap on judged paragraphs per revision — cost control, recorded in the
/// verdict as `checked` so partial coverage is visible, never implied away.
pub const MAX_JUDGED_PARAGRAPHS: usize = 8;

/// Versioned prompt. The one rule that matters most is stated twice: the
/// standard is the MEMORY text, not the model's own knowledge of the world.
pub const JUDGE_SYSTEM_PROMPT_V1: &str = "\
You check whether sentences in a knowledge-base page faithfully restate the memories they cite.

You are given numbered CLAIMS. Each claim is a paragraph followed by the full text of every memory it cites.

For each claim, decide: does the paragraph say only what its cited memories say?
- faithful=false when the paragraph contradicts a cited memory, overstates it (e.g. \"always\" where the memory says \"usually\"), understates a stated caveat, or attributes to the memory something it does not contain.
- Judge ONLY against the memory text given. Your own knowledge of the world is irrelevant; a claim that is wrong about the world but faithful to its memory is faithful=true.
- When faithful=false, give a one-sentence note naming what diverged.

Respond with JSON only: {\"verdicts\":[{\"claim\":1,\"faithful\":true,\"note\":\"\"}, ...]} — one verdict per claim, in order.";

/// One paragraph and the memories it cites — the unit the judge grades.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Claim {
    pub excerpt: String,
    pub cited: Vec<Uuid>,
}

/// Split composed markdown into cited paragraphs. Fenced blocks are skipped
/// whole (diagrams and verbatim evidence are deterministic — there is nothing
/// for a judge to second-guess), as are headings and uncited paragraphs (the
/// citation firewall already handled those).
pub(crate) fn cited_paragraphs(content_md: &str) -> Vec<Claim> {
    let mut claims = Vec::new();
    let mut in_code = false;
    for para in content_md.split("\n\n") {
        // A fence toggle anywhere in the chunk flips code state; a chunk that
        // opens a fence (or sits inside one) is not prose.
        let fences = para.matches("```").count();
        let was_in_code = in_code;
        if fences % 2 == 1 {
            in_code = !in_code;
        }
        if was_in_code || fences > 0 {
            continue;
        }
        let text = para.trim();
        // Headings carry no claims; a bare `<sub>` evidence footer cites its
        // memory but carries no prose to misstate — judging it grades a
        // tautology and spends budget a real paragraph needed.
        if text.is_empty() || text.starts_with('#') || text.starts_with("<sub") {
            continue;
        }
        let cited = extract_citations(text);
        if cited.is_empty() {
            continue;
        }
        claims.push(Claim {
            excerpt: text.to_string(),
            cited,
        });
    }
    claims
}

fn extract_citations(text: &str) -> Vec<Uuid> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("[m:") {
        rest = &rest[i + 3..];
        let Some(j) = rest.find(']') else { break };
        if let Ok(id) = rest[..j].trim().parse::<Uuid>() {
            if !out.contains(&id) {
                out.push(id);
            }
        }
        rest = &rest[j + 1..];
    }
    out
}

/// Take at most `cap` claims, spread evenly across the page rather than
/// front-loaded — the end of a long page must not become a blind spot.
pub(crate) fn sample(claims: Vec<Claim>, cap: usize) -> Vec<Claim> {
    if claims.len() <= cap {
        return claims;
    }
    let step = claims.len() as f64 / cap as f64;
    (0..cap)
        .map(|i| claims[(i as f64 * step) as usize].clone())
        .collect()
}

/// A paragraph the judge flagged, in the shape the verdict JSONB stores.
#[derive(Debug, serde::Serialize)]
pub(crate) struct Flag {
    /// Bounded excerpt — a handle for the reviewer, not the whole paragraph.
    pub excerpt: String,
    /// The first memory the paragraph cites — where the reviewer starts.
    pub memory_id: Uuid,
    pub note: String,
}

/// Parse the judge's JSON strictly against the claims we actually sent. An
/// out-of-range index or non-bool verdict is an error, not a guess — a mangled
/// verdict recorded as if it were real would defeat the point of having one.
pub(crate) fn parse_verdicts(raw: &str, checked: &[Claim]) -> Result<Vec<Flag>> {
    let start = raw.find('{').context("no JSON object in judge output")?;
    let end = raw
        .rfind('}')
        .context("unterminated JSON in judge output")?;
    let v: serde_json::Value = serde_json::from_str(&raw[start..=end])?;
    let verdicts = v
        .get("verdicts")
        .and_then(|x| x.as_array())
        .context("judge output has no `verdicts` array")?;

    let mut flags = Vec::new();
    for verdict in verdicts {
        let idx = verdict
            .get("claim")
            .and_then(|c| c.as_u64())
            .context("verdict without a numeric `claim`")? as usize;
        let claim = idx
            .checked_sub(1)
            .and_then(|i| checked.get(i))
            .with_context(|| format!("verdict for claim {idx}, but only {} sent", checked.len()))?;
        let faithful = verdict
            .get("faithful")
            .and_then(|f| f.as_bool())
            .context("verdict without a boolean `faithful`")?;
        if faithful {
            continue;
        }
        flags.push(Flag {
            excerpt: claim.excerpt.chars().take(240).collect(),
            memory_id: claim.cited[0],
            note: verdict
                .get("note")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .chars()
                .take(300)
                .collect(),
        });
    }
    Ok(flags)
}

/// Judge one revision and record the verdict on it. Returns how many
/// paragraphs were flagged, or `None` when there was nothing to judge (no
/// cited prose — e.g. a pure standards or diagram page).
///
/// Reads happen in one RLS-scoped transaction (the memories the judge sees
/// are exactly the memories the page's audience may see), the model call
/// happens with no transaction held, and the verdict lands in its own write.
pub async fn judge_and_record(
    store: &Store,
    principal: &brainiac_core::Principal,
    provider: &dyn ChatProvider,
    revision_id: Uuid,
) -> Result<Option<usize>> {
    // ── read ────────────────────────────────────────────────────────────
    let (checked, memory_texts) = {
        let mut tx = store.scoped_tx(principal).await?;
        let revision = brainiac_store::documents::get_revision(&mut tx, revision_id)
            .await?
            .context("revision to judge does not exist (or is not visible)")?;
        let claims = sample(
            cited_paragraphs(&revision.content_md),
            MAX_JUDGED_PARAGRAPHS,
        );
        if claims.is_empty() {
            tx.commit().await?;
            return Ok(None);
        }
        let ids: Vec<Uuid> = claims
            .iter()
            .flat_map(|c| c.cited.iter().copied())
            .collect();
        let rows = sqlx::query("SELECT id, content FROM memories WHERE id = ANY($1)")
            .bind(&ids)
            .fetch_all(&mut *tx)
            .await?;
        tx.commit().await?;
        let texts: std::collections::HashMap<Uuid, String> = rows
            .iter()
            .map(|r| (r.get::<Uuid, _>("id"), r.get::<String, _>("content")))
            .collect();
        (claims, texts)
    };

    // A claim whose cited memories are all unreadable under this principal
    // cannot be judged against anything — drop it rather than let the model
    // grade a paragraph against no standard.
    let checked: Vec<Claim> = checked
        .into_iter()
        .filter(|c| c.cited.iter().any(|id| memory_texts.contains_key(id)))
        .collect();
    if checked.is_empty() {
        return Ok(None);
    }

    // ── judge (no transaction held across the model call) ──────────────
    let mut user = String::from("CLAIMS:\n");
    for (i, claim) in checked.iter().enumerate() {
        user.push_str(&format!("{}. {}\n", i + 1, claim.excerpt));
        for id in &claim.cited {
            if let Some(text) = memory_texts.get(id) {
                user.push_str(&format!("   MEMORY m:{id}: {text}\n"));
            }
        }
        user.push('\n');
    }
    let resp = provider
        .complete(&ChatRequest {
            system: JUDGE_SYSTEM_PROMPT_V1.to_string(),
            user,
            json_mode: true,
            max_tokens: 700,
            temperature: 0.0,
        })
        .await?;
    let flags = parse_verdicts(&resp.text, &checked)?;
    let flagged = flags.len();

    // ── record ──────────────────────────────────────────────────────────
    let verdict = serde_json::json!({
        "model_ref": resp.model_ref,
        "checked": checked.len(),
        "flagged": flags,
        "judged_at": chrono::Utc::now().to_rfc3339(),
    });
    let mut tx = store.scoped_tx(principal).await?;
    brainiac_store::documents::set_revision_faithfulness(&mut tx, revision_id, &verdict).await?;
    tx.commit().await?;
    Ok(Some(flagged))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    #[test]
    fn cited_prose_is_extracted_and_fences_and_headings_are_not() {
        let a = uuid(1);
        let b = uuid(2);
        let md = format!(
            "# Page\n\n## How it works\n\nThe cap is 30s [m:{a}].\n\n\
             ```mermaid\ngraph LR\nx --> y\n```\n\n\
             Uncited filler paragraph.\n\nRetries fail open [m:{b}] [m:{a}].\n"
        );
        let claims = cited_paragraphs(&md);
        assert_eq!(claims.len(), 2);
        assert_eq!(claims[0].cited, vec![a]);
        assert_eq!(claims[1].cited, vec![b, a]);
    }

    #[test]
    fn verbatim_evidence_is_never_judged() {
        let a = uuid(3);
        // The evidence_blocks shape: a fenced verbatim artifact plus its
        // `<sub>` citation footer. Neither is a claim — the fence is
        // deterministic copy, the footer is provenance without prose.
        let md = format!("```yaml\nretry: 30s\n```\n\n<sub>[m:{a}]</sub>");
        assert!(cited_paragraphs(&md).is_empty());
    }

    #[test]
    fn sampling_spreads_across_the_page_instead_of_front_loading() {
        let claims: Vec<Claim> = (0..20)
            .map(|i| Claim {
                excerpt: format!("claim {i}"),
                cited: vec![uuid(9)],
            })
            .collect();
        let picked = sample(claims, 4);
        assert_eq!(picked.len(), 4);
        assert_eq!(picked[0].excerpt, "claim 0");
        assert_eq!(picked[3].excerpt, "claim 15");
    }

    #[test]
    fn verdicts_parse_and_only_unfaithful_claims_flag() {
        let claims = vec![
            Claim {
                excerpt: "The cap is 30s.".into(),
                cited: vec![uuid(1)],
            },
            Claim {
                excerpt: "Retries always succeed.".into(),
                cited: vec![uuid(2)],
            },
        ];
        let raw = r#"{"verdicts":[
            {"claim":1,"faithful":true,"note":""},
            {"claim":2,"faithful":false,"note":"memory says retries usually succeed"}
        ]}"#;
        let flags = parse_verdicts(raw, &claims).expect("parse");
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].memory_id, uuid(2));
        assert!(flags[0].note.contains("usually"));
    }

    #[test]
    fn a_verdict_for_a_claim_never_sent_is_an_error_not_a_guess() {
        let claims = vec![Claim {
            excerpt: "x".into(),
            cited: vec![uuid(1)],
        }];
        let raw = r#"{"verdicts":[{"claim":7,"faithful":false,"note":"?"}]}"#;
        assert!(parse_verdicts(raw, &claims).is_err());
    }

    #[test]
    fn chatter_around_the_json_is_tolerated() {
        let claims = vec![Claim {
            excerpt: "x".into(),
            cited: vec![uuid(1)],
        }];
        let raw = "Here is my judgment:\n{\"verdicts\":[{\"claim\":1,\"faithful\":true,\"note\":\"\"}]}\nDone.";
        assert!(parse_verdicts(raw, &claims).expect("parse").is_empty());
    }
}
