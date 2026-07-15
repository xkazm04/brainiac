//! Federated identity → the one project a person owns (migration 0022).
//!
//! This is the free-tier self-serve seam: a verified Google identity is exchanged
//! for exactly one org ("project"), idempotently. It is deliberately the ONLY
//! place that creates an org outside fixture seeding.
//!
//! RUNS ON THE ADMIN POOL, and must. Creating a NEW org cannot happen under a
//! tenant's `scoped_tx`: RLS scopes every write to the caller's existing org, and
//! a person signing up has no org yet — there is nothing to scope to. The
//! provisioning endpoint therefore uses the RLS-bypassing owner pool, and this
//! module is written on the assumption that its caller already did that. The
//! `identities` table has no grant to the runtime role at all, so a mistake here
//! fails closed rather than leaking the account map.

use anyhow::Result;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// The project a person owns, plus whether this call is what created it.
#[derive(Debug, Clone)]
pub struct Provisioned {
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub team_id: Uuid,
    /// False when the identity already had a project — the "one per account"
    /// rule expressed as an outcome rather than an error.
    pub created: bool,
}

/// Look up the project an identity already owns.
pub async fn find(
    conn: &mut PgConnection,
    provider: &str,
    subject: &str,
) -> Result<Option<(Uuid, Uuid)>> {
    let row =
        sqlx::query("SELECT org_id, user_id FROM identities WHERE provider = $1 AND subject = $2")
            .bind(provider)
            .bind(subject)
            .fetch_optional(conn)
            .await?;
    Ok(row.map(|r| (r.get::<Uuid, _>("org_id"), r.get::<Uuid, _>("user_id"))))
}

/// Exchange a VERIFIED identity for its project, creating it on first sight.
///
/// Idempotent: a second sign-in returns the existing project with
/// `created: false`. That is the whole "1 account = 1 project" rule — it is not
/// an error to sign in twice, it just doesn't make a second project.
///
/// The caller MUST have verified `subject`/`email` against the identity provider
/// first. Nothing here can tell a real Google uid from a made-up string; this
/// function's contract is "you proved it, I'll record it".
pub async fn provision(
    conn: &mut PgConnection,
    provider: &str,
    subject: &str,
    email: &str,
    project_name: &str,
) -> Result<Provisioned> {
    // The existing-identity path. Also the concurrent-signup loser's path: the
    // INSERT below can lose the PK race, and we re-read rather than fail.
    if let Some((org_id, user_id)) = find(&mut *conn, provider, subject).await? {
        let team_id = personal_team(&mut *conn, org_id).await?;
        return Ok(Provisioned {
            org_id,
            user_id,
            team_id,
            created: false,
        });
    }

    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();

    crate::orgs::upsert_org(&mut *conn, org_id, project_name).await?;
    crate::orgs::upsert_user(&mut *conn, user_id, org_id, email).await?;
    // One team, and the owner maintains it: every governance gate in the product
    // (promotion review, dispute adjudication, doc publish) checks maintainer of
    // the owning team, so a solo owner who is NOT a maintainer would sign up into
    // a project where they cannot approve their own knowledge.
    crate::orgs::upsert_team(&mut *conn, team_id, org_id, "workspace").await?;
    crate::orgs::upsert_member(&mut *conn, team_id, user_id, "maintainer").await?;

    // ON CONFLICT DO NOTHING + rows_affected is the race guard: two first
    // sign-ins (two tabs, a double-clicked button) both reach here, and the PK
    // lets exactly one win. The loser re-reads the winner's project rather than
    // erroring — the user cannot tell, and no second project exists.
    let inserted = sqlx::query(
        "INSERT INTO identities (provider, subject, user_id, org_id, email)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (provider, subject) DO NOTHING",
    )
    .bind(provider)
    .bind(subject)
    .bind(user_id)
    .bind(org_id)
    .bind(email)
    .execute(&mut *conn)
    .await?
    .rows_affected()
        == 1;

    if !inserted {
        let (won_org, won_user) = find(&mut *conn, provider, subject)
            .await?
            .ok_or_else(|| anyhow::anyhow!("identity vanished after a lost insert race"))?;
        let team_id = personal_team(&mut *conn, won_org).await?;
        return Ok(Provisioned {
            org_id: won_org,
            user_id: won_user,
            team_id,
            created: false,
        });
    }

    Ok(Provisioned {
        org_id,
        user_id,
        team_id,
        created: true,
    })
}

/// The org's workspace team. A self-serve project has exactly one; `ORDER BY id`
/// only makes the answer deterministic if that ever stops being true (the table
/// has no created_at to order by).
async fn personal_team(conn: &mut PgConnection, org_id: Uuid) -> Result<Uuid> {
    let row = sqlx::query("SELECT id FROM teams WHERE org_id = $1 ORDER BY id LIMIT 1")
        .bind(org_id)
        .fetch_optional(conn)
        .await?;
    row.map(|r| r.get::<Uuid, _>("id"))
        .ok_or_else(|| anyhow::anyhow!("project {org_id} has no team"))
}
