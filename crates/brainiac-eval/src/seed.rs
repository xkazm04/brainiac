//! Seed the Meridian fixtures into a live store: identity, gold memories
//! (with embeddings), the entity graph, and GOLD canonical links — the
//! `retrieval` profile starts from ground truth so retrieval quality is
//! measured in isolation (EVAL.md §3, key property).

use anyhow::Result;
use brainiac_core::embed::Embedder;
use brainiac_core::{MemoryKind, MemoryStatus, Principal, Visibility};
use brainiac_fixtures::ids::stable_uuid;
use brainiac_fixtures::Fixtures;
use brainiac_store::{entities, memories, orgs, Store};
use uuid::Uuid;

pub struct Seeded {
    pub org_id: Uuid,
    pub embedding_version: i32,
}

/// Principal used for seeding: an org-scoped writer. RLS INSERT policies only
/// require the org to match; reads during seeding are not relied upon.
pub fn seeding_principal(fx: &Fixtures) -> Principal {
    Principal {
        org_id: stable_uuid(&fx.org.org),
        user_id: stable_uuid("seed-worker"),
        team_ids: vec![],
    }
}

/// Principal for a fixture user (QA `asking_as`).
pub fn principal_for_user(fx: &Fixtures, user_id: &str) -> Option<Principal> {
    let user = fx.org.users.iter().find(|u| u.id == user_id)?;
    Some(Principal {
        org_id: stable_uuid(&fx.org.org),
        user_id: stable_uuid(&user.id),
        team_ids: user.teams.iter().map(|t| stable_uuid(t)).collect(),
    })
}

/// Seed ONLY identity + raw entities for the `resolution` profile. Unlike
/// [`seed_gold`], it deliberately does NOT seed the gold canonical entities or
/// links — those are exactly what the resolve stage must PREDICT, so seeding
/// them would score the fixtures against themselves. Memories/embeddings are
/// irrelevant to entity resolution and are skipped too. Returns the org id.
pub async fn seed_resolution(store: &Store, fx: &Fixtures) -> Result<Uuid> {
    let org_id = stable_uuid(&fx.org.org);
    let p = seeding_principal(fx);
    let mut tx = store.scoped_tx(&p).await?;
    let c = &mut *tx;

    orgs::upsert_org(c, org_id, &fx.org.org).await?;
    for t in &fx.org.teams {
        orgs::upsert_team(c, stable_uuid(&t.id), org_id, &t.name).await?;
    }
    for u in &fx.org.users {
        orgs::upsert_user(c, stable_uuid(&u.id), org_id, &u.email).await?;
        for t in &u.teams {
            orgs::upsert_member(c, stable_uuid(t), stable_uuid(&u.id), &u.role).await?;
        }
    }
    for e in &fx.entities.entities {
        entities::insert_entity(
            c,
            stable_uuid(&e.id),
            org_id,
            Some(stable_uuid(&e.team)),
            &e.name,
            &e.kind,
            &e.aliases,
            None,
        )
        .await?;
    }

    tx.commit().await?;
    Ok(org_id)
}

pub async fn seed_gold(store: &Store, fx: &Fixtures, embedder: &dyn Embedder) -> Result<Seeded> {
    let org_id = stable_uuid(&fx.org.org);
    let p = seeding_principal(fx);
    let mut tx = store.scoped_tx(&p).await?;
    let c = &mut *tx;

    // Identity.
    orgs::upsert_org(c, org_id, &fx.org.org).await?;
    for t in &fx.org.teams {
        orgs::upsert_team(c, stable_uuid(&t.id), org_id, &t.name).await?;
    }
    for u in &fx.org.users {
        orgs::upsert_user(c, stable_uuid(&u.id), org_id, &u.email).await?;
        for t in &u.teams {
            orgs::upsert_member(c, stable_uuid(t), stable_uuid(&u.id), &u.role).await?;
        }
    }

    // Raw entities.
    for e in &fx.entities.entities {
        entities::insert_entity(
            c,
            stable_uuid(&e.id),
            org_id,
            Some(stable_uuid(&e.team)),
            &e.name,
            &e.kind,
            &e.aliases,
            None,
        )
        .await?;
    }

    // GOLD canonical entities + links (retrieval profile bypasses the
    // resolve worker — links are ground truth here).
    for set in &fx.merges.merge_sets {
        let canonical_id = stable_uuid(&format!("canon-{}", set.canonical));
        entities::insert_canonical(c, canonical_id, org_id, &set.canonical, &set.kind).await?;
        for member in &set.members {
            entities::link(c, stable_uuid(member), canonical_id, 1.0, "human", None).await?;
        }
    }

    // Memories + anchors + evidence edges + embeddings.
    let version =
        memories::ensure_embedding_version(c, embedder.model_name(), embedder.dim() as i32).await?;
    for m in &fx.memories.memories {
        let id = stable_uuid(&m.id);
        memories::insert(
            c,
            &memories::NewMemory {
                id,
                org_id,
                team_id: Some(stable_uuid(&m.team)),
                owner_user_id: m.owner.as_ref().map(|o| stable_uuid(o)),
                visibility: Visibility::parse(&m.visibility).unwrap_or(Visibility::Private),
                status: MemoryStatus::parse(&m.status).unwrap_or(MemoryStatus::Canonical),
                kind: MemoryKind::parse(&m.kind).unwrap_or(MemoryKind::Fact),
                content: m.content.clone(),
                language: m.language.clone(),
                valid_from: m.valid_from,
                valid_to: m.valid_to,
                superseded_by: m.superseded_by.as_ref().map(|s| stable_uuid(s)),
                confidence: None,
                provenance_id: None,
            },
        )
        .await?;
        for e in &m.entities {
            memories::link_entity(c, id, stable_uuid(e)).await?;
        }
        for r in &m.relations {
            entities::insert_edge(
                c,
                stable_uuid(&format!("edge-{}-{}-{}", m.id, r.src, r.dst)),
                org_id,
                stable_uuid(&r.src),
                stable_uuid(&r.dst),
                &r.rel,
                Some(id),
            )
            .await?;
        }
        memories::upsert_embedding(c, id, version, &embedder.embed(&m.content).await?).await?;
    }

    tx.commit().await?;
    Ok(Seeded {
        org_id,
        embedding_version: version,
    })
}
