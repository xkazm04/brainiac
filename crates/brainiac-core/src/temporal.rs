//! Temporal validity & supersession logic (ARCHITECTURE.md §2.3).
//!
//! Rules this module is the single owner of:
//! - A memory is *valid at* time `t` when `valid_from <= t` (or unset) and
//!   `valid_to > t` (or unset). `valid_to = NULL` means still valid.
//! - Supersession chains (`superseded_by` forward pointers) resolve to their
//!   head; a retrieval result must never contain both a superseded memory and
//!   its successor for the same point in time.
//! - "As of" queries pick the chain member valid at the asked time — this is
//!   what makes "what did we know in March" answerable.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::types::Memory;

/// Is the memory valid at `t` per its validity window?
pub fn valid_at(memory: &Memory, t: DateTime<Utc>) -> bool {
    if let Some(from) = memory.valid_from {
        if from > t {
            return false;
        }
    }
    if let Some(to) = memory.valid_to {
        if to <= t {
            return false;
        }
    }
    true
}

/// Follow `superseded_by` pointers to the chain head reachable within `pool`.
///
/// Cycle-*canonical*, not merely cycle-terminating: if the walk revisits a node,
/// the members of that cycle all resolve to ONE deterministic head (the min id
/// among the cycle's members). A start-dependent head would put two members of a
/// single cycle into different `chain_of` buckets, and `dedupe_for_time` would
/// then serve both — the exact "a superseded memory and its successor must never
/// coexist" invariant this module owns. Corrupt data degrades to one winner, not
/// a hang and not a double.
pub fn chain_head<'a>(start: &'a Memory, pool: &'a HashMap<Uuid, &'a Memory>) -> &'a Memory {
    let mut current = start;
    let mut seen = vec![current.id];
    while let Some(next_id) = current.superseded_by {
        if let Some(pos) = seen.iter().position(|id| *id == next_id) {
            // Cycle detected. The cycle members are `seen[pos..]` (the tail from
            // the revisited node onward); the prefix `seen[..pos]` is a lead-in
            // that funnels into the same cycle. Pick the min id of the cycle so
            // every start node reaching this cycle returns the same head.
            let rep_id = *seen[pos..].iter().min().expect("cycle slice is non-empty");
            return pool.get(&rep_id).copied().unwrap_or(current);
        }
        match pool.get(&next_id) {
            Some(next) => {
                seen.push(next_id);
                current = next;
            }
            // Pointer leaves the pool → `current` is the reachable head.
            None => break,
        }
    }
    current
}

/// Deduplicate supersession chains for a point-in-time view:
/// for every chain present in `memories`, keep exactly the member that is
/// valid at `as_of` (preferring the newest such member); drop the rest.
/// Memories outside any chain pass through when valid at `as_of`.
///
/// Input order is preserved for the survivors (retrieval rank order matters).
pub fn dedupe_for_time(memories: &[Memory], as_of: DateTime<Utc>) -> Vec<Memory> {
    let pool: HashMap<Uuid, &Memory> = memories.iter().map(|m| (m.id, m)).collect();

    // Group members by chain head id.
    let mut chain_of: HashMap<Uuid, Uuid> = HashMap::new();
    for m in memories {
        let head = chain_head(m, &pool);
        chain_of.insert(m.id, head.id);
    }

    // For each chain, pick the winner: valid at as_of, newest valid_from wins
    // ties (a supersession sets the successor's valid_from at the changeover).
    let mut winner_of_chain: HashMap<Uuid, Uuid> = HashMap::new();
    for m in memories {
        if !valid_at(m, as_of) {
            continue;
        }
        let chain = chain_of[&m.id];
        match winner_of_chain.get(&chain) {
            None => {
                winner_of_chain.insert(chain, m.id);
            }
            Some(existing_id) => {
                let existing = pool[existing_id];
                let newer = match (m.valid_from, existing.valid_from) {
                    (Some(a), Some(b)) => a > b,
                    (Some(_), None) => true,
                    _ => false,
                };
                if newer {
                    winner_of_chain.insert(chain, m.id);
                }
            }
        }
    }

    memories
        .iter()
        .filter(|m| winner_of_chain.get(&chain_of[&m.id]) == Some(&m.id))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MemoryKind, MemoryStatus, Visibility};
    use chrono::TimeZone;

    fn uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn ts(month: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, month, 1, 0, 0, 0)
            .single()
            .expect("valid ts")
    }

    fn mem(id: u8, from: Option<u32>, to: Option<u32>, superseded_by: Option<u8>) -> Memory {
        Memory {
            id: uuid(id),
            org_id: uuid(99),
            team_id: None,
            owner_user_id: None,
            visibility: Visibility::Org,
            status: MemoryStatus::Canonical,
            kind: MemoryKind::Fact,
            content: format!("memory {id}"),
            lifecycle: crate::Lifecycle::Shipped,
            detail_md: None,
            valid_from: from.map(ts),
            valid_to: to.map(ts),
            superseded_by: superseded_by.map(uuid),
            confidence: None,
            provenance_id: None,
            created_at: ts(1),
        }
    }

    #[test]
    fn validity_window_edges() {
        let m = mem(1, Some(3), Some(6), None);
        assert!(!valid_at(&m, ts(2)), "before valid_from");
        assert!(valid_at(&m, ts(3)), "inclusive start");
        assert!(valid_at(&m, ts(5)), "inside window");
        assert!(!valid_at(&m, ts(6)), "exclusive end");
        // Open-ended
        let open = mem(2, None, None, None);
        assert!(valid_at(&open, ts(1)));
    }

    #[test]
    fn as_of_picks_the_temporally_correct_chain_member() {
        // old policy (valid Jan–Apr, superseded by new), new policy (valid from Apr)
        let old = mem(1, Some(1), Some(4), Some(2));
        let new = mem(2, Some(4), None, None);
        let all = vec![old.clone(), new.clone()];

        let march = dedupe_for_time(&all, ts(3));
        assert_eq!(march.len(), 1);
        assert_eq!(
            march[0].id, old.id,
            "as-of March the old policy still holds"
        );

        let june = dedupe_for_time(&all, ts(6));
        assert_eq!(june.len(), 1);
        assert_eq!(june[0].id, new.id, "after supersession only the new policy");
    }

    #[test]
    fn superseded_never_coexists_with_successor() {
        // Overlapping validity (sloppy data): both valid in April — the newer
        // chain member must win, not both.
        let old = mem(1, Some(1), Some(5), Some(2));
        let new = mem(2, Some(4), None, None);
        let out = dedupe_for_time(&[old, new.clone()], ts(4));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, new.id);
    }

    #[test]
    fn unrelated_memories_pass_through_and_keep_order() {
        let a = mem(1, None, None, None);
        let b = mem(2, None, None, None);
        let out = dedupe_for_time(&[a.clone(), b.clone()], ts(3));
        assert_eq!(
            out.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![a.id, b.id]
        );
    }

    #[test]
    fn cyclic_chain_members_share_one_head_and_collapse() {
        // 1 <-> 2 (corrupt cycle). Both members must resolve to the SAME head (the
        // min id) so they land in one chain bucket — not terminate at a
        // start-dependent node that would let both survive dedupe.
        let m1 = mem(1, None, None, Some(2));
        let m2 = mem(2, None, None, Some(1));
        let all = [m1.clone(), m2.clone()];
        let pool: HashMap<Uuid, &Memory> = all.iter().map(|m| (m.id, m)).collect();
        assert_eq!(
            chain_head(&all[0], &pool).id,
            uuid(1),
            "canonical head = min id"
        );
        assert_eq!(
            chain_head(&all[1], &pool).id,
            uuid(1),
            "the other cycle member agrees on the head"
        );
        // Both valid at as_of ⇒ the cycle must collapse to exactly one survivor.
        let out = dedupe_for_time(&[m1, m2], ts(3));
        assert_eq!(
            out.len(),
            1,
            "a supersession cycle must not surface both versions"
        );
    }

    #[test]
    fn self_supersession_resolves_to_itself() {
        // A memory pointing at itself (self-merge corruption) is its own head.
        let m = mem(1, None, None, Some(1));
        let all = [m.clone()];
        let pool: HashMap<Uuid, &Memory> = all.iter().map(|x| (x.id, x)).collect();
        assert_eq!(chain_head(&all[0], &pool).id, uuid(1));
        assert_eq!(dedupe_for_time(&[m], ts(3)).len(), 1);
    }

    #[test]
    fn three_hop_chain_resolves_to_head_only() {
        let a = mem(1, Some(1), Some(2), Some(2));
        let b = mem(2, Some(2), Some(3), Some(3));
        let c = mem(3, Some(3), None, None);
        let out = dedupe_for_time(&[a, b, c.clone()], ts(5));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, c.id);
    }
}
