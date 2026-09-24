//! Supabase / Postgres connection management.

use anyhow::Context;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::PgPool;
use std::time::Duration;

/// The cosmetics schema, embedded so the backend can self-migrate on boot.
pub const COSMETICS_MIGRATION: &str =
    include_str!("../../../../supabase/migrations/0001_cosmetics.sql");

/// Create a Postgres connection pool.
///
/// `Prefer` TLS negotiates encryption with Supabase while still working against
/// a plain local Postgres in development.
pub async fn create_pool(database_url: &str) -> anyhow::Result<PgPool> {
    let options: PgConnectOptions = database_url
        .parse::<PgConnectOptions>()
        .context("DATABASE_URL is not a valid Postgres connection string")?
        .ssl_mode(PgSslMode::Prefer);

    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await
        .context("failed to connect to the database")
}

/// Apply the embedded schema. Idempotent — every statement is `if not exists`
/// or `create or replace`.
pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::raw_sql(COSMETICS_MIGRATION)
        .execute(pool)
        .await
        .context("failed to apply the cosmetics migration")?;
    Ok(())
}

/// Is the database reachable and does it have our schema?
pub async fn health(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query_scalar::<_, i64>("select count(*) from public.cosmetics")
        .fetch_one(pool)
        .await
        .context("cosmetics schema is missing — was the migration applied?")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn migration_is_embedded_and_has_the_core_tables() {
        let sql = super::COSMETICS_MIGRATION;
        for table in [
            "profiles",
            "cosmetics",
            "inventory",
            "equipped",
            "wallets",
            "purchases",
        ] {
            assert!(sql.contains(table), "migration is missing table `{table}`");
        }
        assert!(
            sql.contains("purchase_cosmetic"),
            "purchase function missing"
        );
        assert!(sql.contains("set search_path = public, pg_temp"));
        assert!(sql.contains(
            "revoke execute on function public.purchase_cosmetic(text, uuid, text) from public"
        ));
        assert!(!sql.contains("create policy \"profiles are publicly readable\""));
        assert!(!sql.contains("create policy \"equipped is publicly readable\""));
        assert!(sql.contains("enable row level security"), "RLS missing");
    }
}
