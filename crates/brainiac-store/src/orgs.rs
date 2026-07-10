//! Identity & tenancy writes (fixture seeding + SCIM reconciliation later).
//! Idempotent upserts keyed by primary key so fixture replay is stable.

use anyhow::Result;
use sqlx::PgConnection;
use uuid::Uuid;

pub async fn upsert_org(conn: &mut PgConnection, id: Uuid, name: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO orgs (id, name) VALUES ($1, $2)
         ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name",
    )
    .bind(id)
    .bind(name)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn upsert_team(
    conn: &mut PgConnection,
    id: Uuid,
    org_id: Uuid,
    name: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO teams (id, org_id, name) VALUES ($1, $2, $3)
         ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name",
    )
    .bind(id)
    .bind(org_id)
    .bind(name)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn upsert_user(
    conn: &mut PgConnection,
    id: Uuid,
    org_id: Uuid,
    email: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO users (id, org_id, email) VALUES ($1, $2, $3)
         ON CONFLICT (id) DO UPDATE SET email = EXCLUDED.email",
    )
    .bind(id)
    .bind(org_id)
    .bind(email)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn upsert_member(
    conn: &mut PgConnection,
    team_id: Uuid,
    user_id: Uuid,
    role: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, $3)
         ON CONFLICT (team_id, user_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(team_id)
    .bind(user_id)
    .bind(role)
    .execute(conn)
    .await?;
    Ok(())
}
