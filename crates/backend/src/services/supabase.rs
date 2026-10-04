use anyhow::Context;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::PgPool;
use std::time::Duration;

pub const COSMETICS_MIGRATION: &str =
    include_str!("../../../../supabase/migrations/0001_cosmetics.sql");

pub const AUTH_MIGRATION: &str = include_str!("../../../../supabase/migrations/0002_auth_news.sql");

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

pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    for sql in [COSMETICS_MIGRATION, AUTH_MIGRATION] {
        sqlx::raw_sql(sql)
            .execute(pool)
            .await
            .context("failed to apply an embedded migration")?;
    }
    Ok(())
}

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
        assert!(sql.contains("enable row level security"), "RLS missing");
    }

    #[test]
    fn auth_migration_is_embedded_and_locked_down() {
        let sql = super::AUTH_MIGRATION;
        for table in ["auth_users", "auth_refresh_tokens", "news"] {
            assert!(
                sql.contains(table),
                "auth migration is missing table `{table}`"
            );
        }
        assert!(sql.contains("password_hash"), "password column missing");
        assert!(sql.contains("token_hash"), "refresh token hash missing");
        assert!(sql.contains("enable row level security"), "RLS missing");
        assert!(
            !sql.contains("using (true) on public.auth_users"),
            "auth tables must not be publicly readable"
        );
    }
}
