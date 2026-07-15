//! Fixture-tree integrity validation.
//!
//! Two classes of check:
//! 1. **Referential** — every id mentioned anywhere resolves to a definition.
//! 2. **Semantic** — the gold labels are internally coherent: QA gold must be
//!    visible to its asker (else the item is unanswerable), leak targets must
//!    be INVISIBLE to their asker (else the leak test is vacuous), temporal
//!    expectations must be valid at their `as_of`, supersession pointers and
//!    contradiction directions must agree.
//!
//! Visibility checks reuse `brainiac_core::Principal::can_read` — the same
//! rule the runtime enforces — so fixtures and product can't drift apart.
//!
//! Findings are structured [`Diagnostic`]s (rule id, file, item, message) so
//! the `fixtures lint` CLI can emit machine-readable / CI-annotated output;
//! [`validate`] keeps the original flat-string shape for the loader.

use std::collections::{HashMap, HashSet};
use std::fmt;

use brainiac_core::{Principal, Visibility};
use serde::Serialize;

use crate::ids::stable_uuid;
use crate::loader::Fixtures;
use crate::schema::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// One lint finding, addressable enough to jump to: the fixture file, the
/// offending item (YAML-path-ish, e.g. `memories[m-003]`), a stable rule id
/// for suppression/tracking, and the human message.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub rule: &'static str,
    pub severity: Severity,
    /// Fixture-root-relative file the item lives in.
    pub file: String,
    /// YAML-path-ish locator, e.g. `memories[m-003].team`.
    pub item: String,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} [{}] {}",
            self.file, self.item, self.rule, self.message
        )
    }
}

struct Emitter {
    out: Vec<Diagnostic>,
}

impl Emitter {
    fn err(&mut self, rule: &'static str, file: impl Into<String>, item: String, message: String) {
        self.out.push(Diagnostic {
            rule,
            severity: Severity::Error,
            file: file.into(),
            item,
            message,
        });
    }
}

const F_ORG: &str = "org.yaml";
const F_ENTITIES: &str = "entities/entities.yaml";
const F_MERGES: &str = "entities/merges.yaml";
const F_MEMORIES: &str = "memories/gold.yaml";
const F_TRANSCRIPTS: &str = "transcripts/";
const F_CONTRADICTIONS: &str = "contradictions/cases.yaml";
const F_TEMPORAL: &str = "temporal/asof.yaml";
const F_QA: &str = "retrieval/qa.yaml";
const F_LEAK: &str = "retrieval/leak.yaml";
const F_DOCUMENTS: &str = "documents/pages.yaml";
const F_DRIFT: &str = "drift/docs.yaml";

/// Flat-string view of [`lint`] — the loader's bail-on-invalid contract.
pub fn validate(fx: &Fixtures) -> Vec<String> {
    lint(fx).iter().map(|d| d.to_string()).collect()
}

/// Structured integrity findings; empty = healthy tree.
pub fn lint(fx: &Fixtures) -> Vec<Diagnostic> {
    let mut e = Emitter { out: Vec::new() };

    // ── index definitions ────────────────────────────────────────────────
    let team_ids: HashSet<&str> = fx.org.teams.iter().map(|t| t.id.as_str()).collect();
    let users: HashMap<&str, &UserFx> = fx.org.users.iter().map(|u| (u.id.as_str(), u)).collect();
    let entities: HashMap<&str, &EntityFx> = fx
        .entities
        .entities
        .iter()
        .map(|e| (e.id.as_str(), e))
        .collect();
    let memories: HashMap<&str, &MemoryFx> = fx
        .memories
        .memories
        .iter()
        .map(|m| (m.id.as_str(), m))
        .collect();

    // ── uniqueness ───────────────────────────────────────────────────────
    check_unique(
        fx.org.teams.iter().map(|t| t.id.as_str()),
        "team",
        F_ORG,
        &mut e,
    );
    check_unique(
        fx.org.users.iter().map(|u| u.id.as_str()),
        "user",
        F_ORG,
        &mut e,
    );
    check_unique(
        fx.entities.entities.iter().map(|x| x.id.as_str()),
        "entity",
        F_ENTITIES,
        &mut e,
    );
    check_unique(
        fx.memories.memories.iter().map(|m| m.id.as_str()),
        "memory",
        F_MEMORIES,
        &mut e,
    );
    check_unique(
        fx.contradictions.cases.iter().map(|c| c.id.as_str()),
        "contradiction",
        F_CONTRADICTIONS,
        &mut e,
    );
    check_unique(
        fx.temporal.cases.iter().map(|c| c.id.as_str()),
        "temporal case",
        F_TEMPORAL,
        &mut e,
    );
    check_unique(
        fx.qa.queries.iter().map(|q| q.id.as_str()),
        "qa query",
        F_QA,
        &mut e,
    );
    check_unique(
        fx.leak.queries.iter().map(|q| q.id.as_str()),
        "leak query",
        F_LEAK,
        &mut e,
    );
    check_unique(
        fx.transcripts.iter().map(|t| t.id.as_str()),
        "transcript",
        F_TRANSCRIPTS,
        &mut e,
    );
    check_unique(
        fx.documents.documents.iter().map(|d| d.id.as_str()),
        "document",
        F_DOCUMENTS,
        &mut e,
    );

    // ── composition gold (EVAL §2.6) ─────────────────────────────────────
    // The leak list is the highest-stakes reference in the whole tree: a typo'd
    // memory id there would make the zero-tolerance leak gate PASS VACUOUSLY —
    // the eval would be checking that a memory which does not exist never
    // appears. That is worse than having no gate at all, because it reports
    // safety it never verified.
    for d in &fx.documents.documents {
        if !team_ids.contains(d.team.as_str()) {
            e.err(
                "doc-team-unknown",
                F_DOCUMENTS,
                d.id.clone(),
                format!("unknown team `{}`", d.team),
            );
        }
        for fm in &d.forbidden_memories {
            if !memories.contains_key(fm.as_str()) {
                e.err(
                    "doc-forbidden-unknown",
                    F_DOCUMENTS,
                    d.id.clone(),
                    format!(
                        "forbidden_memories references unknown memory `{fm}` — the leak gate \
                         would pass vacuously"
                    ),
                );
            }
        }
        if let Some(sc) = &d.staleness_case {
            for m in [&sc.supersede.old, &sc.supersede.new] {
                if !memories.contains_key(m.as_str()) {
                    e.err(
                        "doc-staleness-unknown",
                        F_DOCUMENTS,
                        d.id.clone(),
                        format!("staleness_case references unknown memory `{m}`"),
                    );
                }
            }
        }
        for s in &d.sections {
            match s.mode.as_str() {
                "composed" => {
                    if s.bindings.is_none() {
                        e.err(
                            "doc-section-shape",
                            F_DOCUMENTS,
                            d.id.clone(),
                            format!("composed section `{}` has no bindings", s.heading),
                        );
                    }
                    if let Some(b) = &s.bindings {
                        for ent in &b.entities {
                            if !entities.contains_key(ent.as_str()) {
                                e.err(
                                    "doc-binding-entity-unknown",
                                    F_DOCUMENTS,
                                    d.id.clone(),
                                    format!("binding references unknown entity `{ent}`"),
                                );
                            }
                        }
                    }
                }
                "pinned" => {
                    if s.pinned_content.is_none() {
                        e.err(
                            "doc-section-shape",
                            F_DOCUMENTS,
                            d.id.clone(),
                            format!("pinned section `{}` has no content", s.heading),
                        );
                    }
                }
                other => e.err(
                    "doc-section-mode",
                    F_DOCUMENTS,
                    d.id.clone(),
                    format!("unknown section mode `{other}`"),
                ),
            }
        }
    }

    // ── docs-drift gold (Level 2) ────────────────────────────────────────
    // The `propose` pointer is this profile's leak-list equivalent: a typo'd id
    // would make proposal accuracy score against a memory that does not exist.
    check_unique(
        fx.drift.docs.iter().map(|d| d.id.as_str()),
        "drift doc",
        F_DRIFT,
        &mut e,
    );
    for d in &fx.drift.docs {
        for (i, g) in d.gold.iter().enumerate() {
            let item = format!("{}[{i}]", d.id);
            if !d.body.contains(g.claim.as_str()) {
                e.err(
                    "drift-claim-missing",
                    F_DRIFT,
                    item.clone(),
                    format!(
                        "gold claim `{}` is not a substring of the doc body",
                        g.claim
                    ),
                );
            }
            match g.label.as_str() {
                "drifted" => match &g.propose {
                    None => e.err(
                        "drift-propose-missing",
                        F_DRIFT,
                        item,
                        "a drifted claim must name the memory to propose".into(),
                    ),
                    Some(p) => {
                        match memories.get(p.as_str()) {
                            None => e.err(
                                "drift-propose-unknown",
                                F_DRIFT,
                                item,
                                format!("propose references unknown memory `{p}`"),
                            ),
                            // Proposing a superseded memory would send the doc
                            // author from one stale belief to another.
                            Some(m) if m.superseded_by.is_some() => e.err(
                                "drift-propose-stale",
                                F_DRIFT,
                                item,
                                format!("propose `{p}` is itself superseded"),
                            ),
                            Some(_) => {}
                        }
                    }
                },
                "aligned" | "unmatched" => {
                    if g.propose.is_some() {
                        e.err(
                            "drift-propose-shape",
                            F_DRIFT,
                            item,
                            format!("a `{}` claim must not carry a proposal", g.label),
                        );
                    }
                }
                other => e.err(
                    "drift-label",
                    F_DRIFT,
                    item,
                    format!("unknown label `{other}` (drifted | aligned | unmatched)"),
                ),
            }
        }
    }

    // Stable-uuid collision check across every id namespace the seeders persist.
    //
    // `stable_uuid` is a namespace-FLAT hash of the raw id string, so a collision
    // between any two ids — even across types (a document id vs a memory id) —
    // maps two distinct fixture entities onto one primary key, and one silently
    // overwrites the other at seed time. That makes cross-namespace coverage the
    // whole point: this previously stopped at teams/users/entities/memories while
    // documents, transcripts, contradictions, temporal, qa and leak ids are all
    // persisted too (docs_profile/extraction_profile/pipeline_profile all call
    // stable_uuid on them).
    //
    // Iterate the underlying Vecs, never HashMap::keys(), so both detection and
    // the emitted diagnostic order are deterministic across runs — a lint whose
    // output reorders is not diffable in CI.
    {
        let mut seen: HashMap<uuid::Uuid, &str> = HashMap::new();
        for id in fx
            .org
            .teams
            .iter()
            .map(|t| t.id.as_str())
            .chain(fx.org.users.iter().map(|u| u.id.as_str()))
            .chain(fx.entities.entities.iter().map(|e| e.id.as_str()))
            .chain(fx.memories.memories.iter().map(|m| m.id.as_str()))
            .chain(fx.documents.documents.iter().map(|d| d.id.as_str()))
            .chain(fx.transcripts.iter().map(|t| t.id.as_str()))
            .chain(fx.contradictions.cases.iter().map(|c| c.id.as_str()))
            .chain(fx.temporal.cases.iter().map(|c| c.id.as_str()))
            .chain(fx.qa.queries.iter().map(|q| q.id.as_str()))
            .chain(fx.leak.queries.iter().map(|q| q.id.as_str()))
        {
            if let Some(prev) = seen.insert(stable_uuid(id), id) {
                e.err(
                    "uuid-collision",
                    F_ORG,
                    format!("ids[{id}]"),
                    format!("stable_uuid collision between `{prev}` and `{id}`"),
                );
            }
        }
    }

    // ── users reference teams ────────────────────────────────────────────
    for u in &fx.org.users {
        for t in &u.teams {
            if !team_ids.contains(t.as_str()) {
                e.err(
                    "unknown-team",
                    F_ORG,
                    format!("users[{}].teams", u.id),
                    format!("user `{}` references unknown team `{t}`", u.id),
                );
            }
        }
    }

    // ── entities reference teams ─────────────────────────────────────────
    for x in &fx.entities.entities {
        if !team_ids.contains(x.team.as_str()) {
            e.err(
                "unknown-team",
                F_ENTITIES,
                format!("entities[{}].team", x.id),
                format!("entity `{}` references unknown team `{}`", x.id, x.team),
            );
        }
    }

    // ── merges ───────────────────────────────────────────────────────────
    let mut member_of: HashMap<&str, &str> = HashMap::new();
    for set in &fx.merges.merge_sets {
        for m in &set.members {
            if !entities.contains_key(m.as_str()) {
                e.err(
                    "unknown-entity",
                    F_MERGES,
                    format!("merge_sets[{}].members", set.canonical),
                    format!(
                        "merge set `{}` references unknown entity `{m}`",
                        set.canonical
                    ),
                );
            }
            if let Some(other) = member_of.insert(m.as_str(), set.canonical.as_str()) {
                e.err(
                    "merge-overlap",
                    F_MERGES,
                    format!("merge_sets[{}].members[{m}]", set.canonical),
                    format!(
                        "entity `{m}` appears in two merge sets: `{other}` and `{}`",
                        set.canonical
                    ),
                );
            }
        }
    }
    for pair in &fx.merges.negative_pairs {
        for m in pair {
            if !entities.contains_key(m.as_str()) {
                e.err(
                    "unknown-entity",
                    F_MERGES,
                    format!("negative_pairs[{},{}]", pair[0], pair[1]),
                    format!("negative pair references unknown entity `{m}`"),
                );
            }
        }
        if let (Some(a), Some(b)) = (
            member_of.get(pair[0].as_str()),
            member_of.get(pair[1].as_str()),
        ) {
            if a == b {
                e.err(
                    "negative-pair-conflict",
                    F_MERGES,
                    format!("negative_pairs[{},{}]", pair[0], pair[1]),
                    format!(
                        "negative pair ({}, {}) is contradicted by merge set `{a}` — gold is inconsistent",
                        pair[0], pair[1]
                    ),
                );
            }
        }
    }

    // ── memories ─────────────────────────────────────────────────────────
    let transcript_ids: HashSet<&str> = fx.transcripts.iter().map(|t| t.id.as_str()).collect();
    for m in &fx.memories.memories {
        let at = |field: &str| format!("memories[{}].{field}", m.id);
        if !team_ids.contains(m.team.as_str()) {
            e.err(
                "unknown-team",
                F_MEMORIES,
                at("team"),
                format!("memory `{}` references unknown team `{}`", m.id, m.team),
            );
        }
        if Visibility::parse(&m.visibility).is_none() {
            e.err(
                "invalid-enum",
                F_MEMORIES,
                at("visibility"),
                format!(
                    "memory `{}` has invalid visibility `{}`",
                    m.id, m.visibility
                ),
            );
        }
        if brainiac_core::MemoryKind::parse(&m.kind).is_none() {
            e.err(
                "invalid-enum",
                F_MEMORIES,
                at("kind"),
                format!("memory `{}` has invalid kind `{}`", m.id, m.kind),
            );
        }
        if brainiac_core::MemoryStatus::parse(&m.status).is_none() {
            e.err(
                "invalid-enum",
                F_MEMORIES,
                at("status"),
                format!("memory `{}` has invalid status `{}`", m.id, m.status),
            );
        }
        if m.visibility == "private" && m.owner.is_none() {
            e.err(
                "missing-owner",
                F_MEMORIES,
                at("owner"),
                format!("private memory `{}` has no owner", m.id),
            );
        }
        if let Some(owner) = &m.owner {
            if !users.contains_key(owner.as_str()) {
                e.err(
                    "unknown-user",
                    F_MEMORIES,
                    at("owner"),
                    format!("memory `{}` owner `{owner}` is not a defined user", m.id),
                );
            }
        }
        for x in &m.entities {
            if !entities.contains_key(x.as_str()) {
                e.err(
                    "unknown-entity",
                    F_MEMORIES,
                    at("entities"),
                    format!("memory `{}` references unknown entity `{x}`", m.id),
                );
            }
        }
        for r in &m.relations {
            for end in [&r.src, &r.dst] {
                if !entities.contains_key(end.as_str()) {
                    e.err(
                        "unknown-entity",
                        F_MEMORIES,
                        at("relations"),
                        format!(
                            "memory `{}` relation references unknown entity `{end}`",
                            m.id
                        ),
                    );
                }
            }
        }
        if let Some(sup) = &m.superseded_by {
            if !memories.contains_key(sup.as_str()) {
                e.err(
                    "unknown-memory",
                    F_MEMORIES,
                    at("superseded_by"),
                    format!("memory `{}` superseded_by unknown memory `{sup}`", m.id),
                );
            }
            if m.valid_to.is_none() {
                e.err(
                    "supersession",
                    F_MEMORIES,
                    at("valid_to"),
                    format!("memory `{}` is superseded but has no valid_to", m.id),
                );
            }
        }
        if let (Some(from), Some(to)) = (m.valid_from, m.valid_to) {
            if from >= to {
                e.err(
                    "temporal-window",
                    F_MEMORIES,
                    at("valid_from"),
                    format!("memory `{}` has valid_from >= valid_to", m.id),
                );
            }
        }
        if let Some(src) = &m.source {
            if !transcript_ids.contains(src.as_str()) {
                e.err(
                    "unknown-transcript",
                    F_MEMORIES,
                    at("source"),
                    format!("memory `{}` references unknown transcript `{src}`", m.id),
                );
            }
        }
    }

    // ── transcripts ──────────────────────────────────────────────────────
    for t in &fx.transcripts {
        let at = |field: &str| format!("transcript[{}].{field}", t.id);
        if !team_ids.contains(t.team.as_str()) {
            e.err(
                "unknown-team",
                F_TRANSCRIPTS,
                at("team"),
                format!("transcript `{}` references unknown team `{}`", t.id, t.team),
            );
        }
        if t.turns.is_empty() {
            e.err(
                "empty-transcript",
                F_TRANSCRIPTS,
                at("turns"),
                format!("transcript `{}` has no turns", t.id),
            );
        }
        for g in &t.gold_memories {
            match memories.get(g.id.as_str()) {
                None => e.err(
                    "unknown-memory",
                    F_TRANSCRIPTS,
                    at(&format!("gold_memories[{}]", g.id)),
                    format!(
                        "transcript `{}` gold memory `{}` is not defined in memories/gold.yaml",
                        t.id, g.id
                    ),
                ),
                Some(m) => {
                    if m.team != t.team {
                        e.err(
                            "transcript-gold",
                            F_TRANSCRIPTS,
                            at(&format!("gold_memories[{}]", g.id)),
                            format!(
                                "transcript `{}` (team {}) gold memory `{}` belongs to team {}",
                                t.id, t.team, g.id, m.team
                            ),
                        );
                    }
                    if m.source.as_deref() != Some(t.id.as_str()) {
                        e.err(
                            "transcript-gold",
                            F_MEMORIES,
                            format!("memories[{}].source", g.id),
                            format!(
                                "gold memory `{}` should declare source `{}` (bidirectional link)",
                                g.id, t.id
                            ),
                        );
                    }
                }
            }
            for x in &g.entities {
                if !entities.contains_key(x.as_str()) {
                    e.err(
                        "unknown-entity",
                        F_TRANSCRIPTS,
                        at(&format!("gold_memories[{}].entities", g.id)),
                        format!(
                            "transcript `{}` gold `{}` references unknown entity `{x}`",
                            t.id, g.id
                        ),
                    );
                }
            }
        }
    }

    // ── contradictions ───────────────────────────────────────────────────
    for c in &fx.contradictions.cases {
        let at = |field: &str| format!("cases[{}].{field}", c.id);
        for m in [&c.memory_a, &c.memory_b] {
            if !memories.contains_key(m.as_str()) {
                e.err(
                    "unknown-memory",
                    F_CONTRADICTIONS,
                    at("memory_a/b"),
                    format!("contradiction `{}` references unknown memory `{m}`", c.id),
                );
            }
        }
        match c.expected.as_str() {
            "resolved_supersede" => {
                let dir = c.supersede_direction.as_deref();
                if dir != Some("a_over_b") && dir != Some("b_over_a") {
                    e.err(
                        "contradiction-direction",
                        F_CONTRADICTIONS,
                        at("supersede_direction"),
                        format!(
                            "contradiction `{}` supersede case missing/invalid direction",
                            c.id
                        ),
                    );
                } else if let (Some(a), Some(b)) = (
                    memories.get(c.memory_a.as_str()),
                    memories.get(c.memory_b.as_str()),
                ) {
                    // The gold corpus must already encode the winning pointer.
                    let (loser, winner) = if dir == Some("b_over_a") {
                        (a, b)
                    } else {
                        (b, a)
                    };
                    if loser.superseded_by.as_deref() != Some(winner.id.as_str()) {
                        e.err(
                            "contradiction-direction",
                            F_CONTRADICTIONS,
                            at("supersede_direction"),
                            format!(
                                "contradiction `{}`: `{}` should carry superseded_by `{}` to match direction",
                                c.id, loser.id, winner.id
                            ),
                        );
                    }
                }
            }
            "resolved_coexist" | "dismissed" => {}
            other => e.err(
                "invalid-enum",
                F_CONTRADICTIONS,
                at("expected"),
                format!("contradiction `{}` has invalid expected `{other}`", c.id),
            ),
        }
    }

    // ── temporal ─────────────────────────────────────────────────────────
    for t in &fx.temporal.cases {
        match memories.get(t.expected_memory.as_str()) {
            None => e.err(
                "unknown-memory",
                F_TEMPORAL,
                format!("cases[{}].expected_memory", t.id),
                format!(
                    "temporal `{}` expects unknown memory `{}`",
                    t.id, t.expected_memory
                ),
            ),
            Some(m) => {
                let from_ok = m.valid_from.is_none_or(|f| f <= t.as_of);
                let to_ok = m.valid_to.is_none_or(|to| to > t.as_of);
                if !from_ok || !to_ok {
                    e.err(
                        "temporal-window",
                        F_TEMPORAL,
                        format!("cases[{}].as_of", t.id),
                        format!(
                            "temporal `{}`: expected memory `{}` is not valid at {}",
                            t.id, t.expected_memory, t.as_of
                        ),
                    );
                }
            }
        }
    }

    // ── principals for QA / leak ─────────────────────────────────────────
    let principal_for = |asking: &AskingAsFx| -> Option<Principal> {
        let user = users.get(asking.user.as_str())?;
        if !user.teams.contains(&asking.team) {
            return None;
        }
        Some(Principal {
            org_id: stable_uuid(&fx.org.org),
            user_id: stable_uuid(&user.id),
            team_ids: user.teams.iter().map(|t| stable_uuid(t)).collect(),
        })
    };
    let memory_scope = |m: &MemoryFx| {
        (
            stable_uuid(&fx.org.org),
            Some(stable_uuid(&m.team)),
            m.owner.as_ref().map(|o| stable_uuid(o)),
            Visibility::parse(&m.visibility).unwrap_or(Visibility::Private),
        )
    };

    for q in &fx.qa.queries {
        let at = |field: &str| format!("queries[{}].{field}", q.id);
        let Some(p) = principal_for(&q.asking_as) else {
            e.err(
                "asking-as",
                F_QA,
                at("asking_as"),
                format!(
                    "qa `{}` asking_as is invalid (unknown user or user not in team)",
                    q.id
                ),
            );
            continue;
        };
        for g in &q.relevant {
            match memories.get(g.memory.as_str()) {
                None => e.err(
                    "unknown-memory",
                    F_QA,
                    at("relevant"),
                    format!("qa `{}` references unknown memory `{}`", q.id, g.memory),
                ),
                Some(m) => {
                    if !(1..=3).contains(&g.grade) {
                        e.err(
                            "qa-grade",
                            F_QA,
                            at(&format!("relevant[{}].grade", g.memory)),
                            format!("qa `{}` grade for `{}` out of range 1..=3", q.id, g.memory),
                        );
                    }
                    let (org, team, owner, vis) = memory_scope(m);
                    if !p.can_read(org, team, owner, vis) {
                        e.err(
                            "qa-visibility",
                            F_QA,
                            at(&format!("relevant[{}]", g.memory)),
                            format!(
                                "qa `{}`: gold memory `{}` is NOT visible to asker `{}` — unanswerable",
                                q.id, g.memory, q.asking_as.user
                            ),
                        );
                    }
                }
            }
        }
        for f in &q.forbidden_top3 {
            if !memories.contains_key(f.as_str()) {
                e.err(
                    "unknown-memory",
                    F_QA,
                    at("forbidden_top3"),
                    format!(
                        "qa `{}` forbidden_top3 references unknown memory `{f}`",
                        q.id
                    ),
                );
            }
        }
        if q.stratum == "negative" && !q.relevant.is_empty() {
            e.err(
                "qa-negative",
                F_QA,
                at("relevant"),
                format!(
                    "qa `{}` is negative-stratum but lists relevant memories",
                    q.id
                ),
            );
        }
    }

    for q in &fx.leak.queries {
        let at = |field: &str| format!("queries[{}].{field}", q.id);
        let Some(p) = principal_for(&q.asking_as) else {
            e.err(
                "asking-as",
                F_LEAK,
                at("asking_as"),
                format!("leak `{}` asking_as is invalid", q.id),
            );
            continue;
        };
        if q.forbidden.is_empty() {
            e.err(
                "leak-vacuous",
                F_LEAK,
                at("forbidden"),
                format!("leak `{}` has no forbidden memories — vacuous", q.id),
            );
        }
        for f in &q.forbidden {
            match memories.get(f.as_str()) {
                None => e.err(
                    "unknown-memory",
                    F_LEAK,
                    at("forbidden"),
                    format!("leak `{}` forbids unknown memory `{f}`", q.id),
                ),
                Some(m) => {
                    let (org, team, owner, vis) = memory_scope(m);
                    if p.can_read(org, team, owner, vis) {
                        e.err(
                            "leak-vacuous",
                            F_LEAK,
                            at(&format!("forbidden[{f}]")),
                            format!(
                                "leak `{}`: asker `{}` CAN legitimately read `{f}` — the leak test is vacuous",
                                q.id, q.asking_as.user
                            ),
                        );
                    }
                }
            }
        }
    }

    e.out
}

fn check_unique<'a>(
    ids: impl Iterator<Item = &'a str>,
    what: &str,
    file: &'static str,
    e: &mut Emitter,
) {
    let mut seen: HashSet<&str> = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            e.err(
                "duplicate-id",
                file,
                format!("{what}[{id}]"),
                format!("duplicate {what} id `{id}`"),
            );
        }
    }
}
