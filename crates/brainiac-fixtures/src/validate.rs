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

use std::collections::{HashMap, HashSet};

use brainiac_core::{Principal, Visibility};

use crate::ids::stable_uuid;
use crate::loader::Fixtures;
use crate::schema::*;

pub fn validate(fx: &Fixtures) -> Vec<String> {
    let mut issues: Vec<String> = Vec::new();
    let mut push = |msg: String| issues.push(msg);

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
        &mut push,
    );
    check_unique(
        fx.org.users.iter().map(|u| u.id.as_str()),
        "user",
        &mut push,
    );
    check_unique(
        fx.entities.entities.iter().map(|e| e.id.as_str()),
        "entity",
        &mut push,
    );
    check_unique(
        fx.memories.memories.iter().map(|m| m.id.as_str()),
        "memory",
        &mut push,
    );
    check_unique(
        fx.contradictions.cases.iter().map(|c| c.id.as_str()),
        "contradiction",
        &mut push,
    );
    check_unique(
        fx.temporal.cases.iter().map(|c| c.id.as_str()),
        "temporal case",
        &mut push,
    );
    check_unique(
        fx.qa.queries.iter().map(|q| q.id.as_str()),
        "qa query",
        &mut push,
    );
    check_unique(
        fx.leak.queries.iter().map(|q| q.id.as_str()),
        "leak query",
        &mut push,
    );
    check_unique(
        fx.transcripts.iter().map(|t| t.id.as_str()),
        "transcript",
        &mut push,
    );

    // Stable-uuid collision check across every id namespace we persist.
    {
        let mut seen: HashMap<uuid::Uuid, &str> = HashMap::new();
        for id in fx
            .org
            .teams
            .iter()
            .map(|t| t.id.as_str())
            .chain(fx.org.users.iter().map(|u| u.id.as_str()))
            .chain(entities.keys().copied())
            .chain(memories.keys().copied())
        {
            if let Some(prev) = seen.insert(stable_uuid(id), id) {
                push(format!("stable_uuid collision between `{prev}` and `{id}`"));
            }
        }
    }

    // ── users reference teams ────────────────────────────────────────────
    for u in &fx.org.users {
        for t in &u.teams {
            if !team_ids.contains(t.as_str()) {
                push(format!("user `{}` references unknown team `{t}`", u.id));
            }
        }
    }

    // ── entities reference teams ─────────────────────────────────────────
    for e in &fx.entities.entities {
        if !team_ids.contains(e.team.as_str()) {
            push(format!(
                "entity `{}` references unknown team `{}`",
                e.id, e.team
            ));
        }
    }

    // ── merges ───────────────────────────────────────────────────────────
    let mut member_of: HashMap<&str, &str> = HashMap::new();
    for set in &fx.merges.merge_sets {
        for m in &set.members {
            if !entities.contains_key(m.as_str()) {
                push(format!(
                    "merge set `{}` references unknown entity `{m}`",
                    set.canonical
                ));
            }
            if let Some(other) = member_of.insert(m.as_str(), set.canonical.as_str()) {
                push(format!(
                    "entity `{m}` appears in two merge sets: `{other}` and `{}`",
                    set.canonical
                ));
            }
        }
    }
    for pair in &fx.merges.negative_pairs {
        for m in pair {
            if !entities.contains_key(m.as_str()) {
                push(format!("negative pair references unknown entity `{m}`"));
            }
        }
        if let (Some(a), Some(b)) = (
            member_of.get(pair[0].as_str()),
            member_of.get(pair[1].as_str()),
        ) {
            if a == b {
                push(format!(
                    "negative pair ({}, {}) is contradicted by merge set `{a}` — gold is inconsistent",
                    pair[0], pair[1]
                ));
            }
        }
    }

    // ── memories ─────────────────────────────────────────────────────────
    let transcript_ids: HashSet<&str> = fx.transcripts.iter().map(|t| t.id.as_str()).collect();
    for m in &fx.memories.memories {
        if !team_ids.contains(m.team.as_str()) {
            push(format!(
                "memory `{}` references unknown team `{}`",
                m.id, m.team
            ));
        }
        if Visibility::parse(&m.visibility).is_none() {
            push(format!(
                "memory `{}` has invalid visibility `{}`",
                m.id, m.visibility
            ));
        }
        if brainiac_core::MemoryKind::parse(&m.kind).is_none() {
            push(format!("memory `{}` has invalid kind `{}`", m.id, m.kind));
        }
        if brainiac_core::MemoryStatus::parse(&m.status).is_none() {
            push(format!(
                "memory `{}` has invalid status `{}`",
                m.id, m.status
            ));
        }
        if m.visibility == "private" && m.owner.is_none() {
            push(format!("private memory `{}` has no owner", m.id));
        }
        if let Some(owner) = &m.owner {
            if !users.contains_key(owner.as_str()) {
                push(format!(
                    "memory `{}` owner `{owner}` is not a defined user",
                    m.id
                ));
            }
        }
        for e in &m.entities {
            if !entities.contains_key(e.as_str()) {
                push(format!("memory `{}` references unknown entity `{e}`", m.id));
            }
        }
        for r in &m.relations {
            for end in [&r.src, &r.dst] {
                if !entities.contains_key(end.as_str()) {
                    push(format!(
                        "memory `{}` relation references unknown entity `{end}`",
                        m.id
                    ));
                }
            }
        }
        if let Some(sup) = &m.superseded_by {
            if !memories.contains_key(sup.as_str()) {
                push(format!(
                    "memory `{}` superseded_by unknown memory `{sup}`",
                    m.id
                ));
            }
            if m.valid_to.is_none() {
                push(format!(
                    "memory `{}` is superseded but has no valid_to",
                    m.id
                ));
            }
        }
        if let (Some(from), Some(to)) = (m.valid_from, m.valid_to) {
            if from >= to {
                push(format!("memory `{}` has valid_from >= valid_to", m.id));
            }
        }
        if let Some(src) = &m.source {
            if !transcript_ids.contains(src.as_str()) {
                push(format!(
                    "memory `{}` references unknown transcript `{src}`",
                    m.id
                ));
            }
        }
    }

    // ── transcripts ──────────────────────────────────────────────────────
    for t in &fx.transcripts {
        if !team_ids.contains(t.team.as_str()) {
            push(format!(
                "transcript `{}` references unknown team `{}`",
                t.id, t.team
            ));
        }
        if t.turns.is_empty() {
            push(format!("transcript `{}` has no turns", t.id));
        }
        for g in &t.gold_memories {
            match memories.get(g.id.as_str()) {
                None => push(format!(
                    "transcript `{}` gold memory `{}` is not defined in memories/gold.yaml",
                    t.id, g.id
                )),
                Some(m) => {
                    if m.team != t.team {
                        push(format!(
                            "transcript `{}` (team {}) gold memory `{}` belongs to team {}",
                            t.id, t.team, g.id, m.team
                        ));
                    }
                    if m.source.as_deref() != Some(t.id.as_str()) {
                        push(format!(
                            "gold memory `{}` should declare source `{}` (bidirectional link)",
                            g.id, t.id
                        ));
                    }
                }
            }
            for e in &g.entities {
                if !entities.contains_key(e.as_str()) {
                    push(format!(
                        "transcript `{}` gold `{}` references unknown entity `{e}`",
                        t.id, g.id
                    ));
                }
            }
        }
    }

    // ── contradictions ───────────────────────────────────────────────────
    for c in &fx.contradictions.cases {
        for m in [&c.memory_a, &c.memory_b] {
            if !memories.contains_key(m.as_str()) {
                push(format!(
                    "contradiction `{}` references unknown memory `{m}`",
                    c.id
                ));
            }
        }
        match c.expected.as_str() {
            "resolved_supersede" => {
                let dir = c.supersede_direction.as_deref();
                if dir != Some("a_over_b") && dir != Some("b_over_a") {
                    push(format!(
                        "contradiction `{}` supersede case missing/invalid direction",
                        c.id
                    ));
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
                        push(format!(
                            "contradiction `{}`: `{}` should carry superseded_by `{}` to match direction",
                            c.id, loser.id, winner.id
                        ));
                    }
                }
            }
            "resolved_coexist" | "dismissed" => {}
            other => push(format!(
                "contradiction `{}` has invalid expected `{other}`",
                c.id
            )),
        }
    }

    // ── temporal ─────────────────────────────────────────────────────────
    for t in &fx.temporal.cases {
        match memories.get(t.expected_memory.as_str()) {
            None => push(format!(
                "temporal `{}` expects unknown memory `{}`",
                t.id, t.expected_memory
            )),
            Some(m) => {
                let from_ok = m.valid_from.is_none_or(|f| f <= t.as_of);
                let to_ok = m.valid_to.is_none_or(|to| to > t.as_of);
                if !from_ok || !to_ok {
                    push(format!(
                        "temporal `{}`: expected memory `{}` is not valid at {}",
                        t.id, t.expected_memory, t.as_of
                    ));
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
        let Some(p) = principal_for(&q.asking_as) else {
            push(format!(
                "qa `{}` asking_as is invalid (unknown user or user not in team)",
                q.id
            ));
            continue;
        };
        for g in &q.relevant {
            match memories.get(g.memory.as_str()) {
                None => push(format!(
                    "qa `{}` references unknown memory `{}`",
                    q.id, g.memory
                )),
                Some(m) => {
                    if !(1..=3).contains(&g.grade) {
                        push(format!(
                            "qa `{}` grade for `{}` out of range 1..=3",
                            q.id, g.memory
                        ));
                    }
                    let (org, team, owner, vis) = memory_scope(m);
                    if !p.can_read(org, team, owner, vis) {
                        push(format!(
                            "qa `{}`: gold memory `{}` is NOT visible to asker `{}` — unanswerable",
                            q.id, g.memory, q.asking_as.user
                        ));
                    }
                }
            }
        }
        for f in &q.forbidden_top3 {
            if !memories.contains_key(f.as_str()) {
                push(format!(
                    "qa `{}` forbidden_top3 references unknown memory `{f}`",
                    q.id
                ));
            }
        }
        if q.stratum == "negative" && !q.relevant.is_empty() {
            push(format!(
                "qa `{}` is negative-stratum but lists relevant memories",
                q.id
            ));
        }
    }

    for q in &fx.leak.queries {
        let Some(p) = principal_for(&q.asking_as) else {
            push(format!("leak `{}` asking_as is invalid", q.id));
            continue;
        };
        if q.forbidden.is_empty() {
            push(format!(
                "leak `{}` has no forbidden memories — vacuous",
                q.id
            ));
        }
        for f in &q.forbidden {
            match memories.get(f.as_str()) {
                None => push(format!("leak `{}` forbids unknown memory `{f}`", q.id)),
                Some(m) => {
                    let (org, team, owner, vis) = memory_scope(m);
                    if p.can_read(org, team, owner, vis) {
                        push(format!(
                            "leak `{}`: asker `{}` CAN legitimately read `{f}` — the leak test is vacuous",
                            q.id, q.asking_as.user
                        ));
                    }
                }
            }
        }
    }

    issues
}

fn check_unique<'a>(ids: impl Iterator<Item = &'a str>, what: &str, push: &mut impl FnMut(String)) {
    let mut seen: HashSet<&str> = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            push(format!("duplicate {what} id `{id}`"));
        }
    }
}
