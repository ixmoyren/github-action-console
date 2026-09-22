use std::path::Path;

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use thiserror::Error;

use crate::github::Account;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database failure: {0}")]
    Database(#[from] sqlx::Error),
}

/// Local SQLite storage. Holds account metadata and preferences only —
/// tokens live in the OS keyring, and release definitions live in the repo
/// manifest (ADR-0003).
#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        Self::from_pool(pool).await
    }

    pub async fn in_memory() -> Result<Self, StoreError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        Self::from_pool(pool).await
    }

    async fn from_pool(pool: SqlitePool) -> Result<Self, StoreError> {
        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    async fn migrate(&self) -> Result<(), StoreError> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS account (
                login TEXT PRIMARY KEY,
                host TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS preference (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn save_account(&self, account: &Account, host: &str) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO account (login, host) VALUES (?, ?)
             ON CONFLICT(login) DO UPDATE SET host = excluded.host",
        )
        .bind(&account.login)
        .bind(host)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_account(&self) -> Result<Option<Account>, StoreError> {
        let login: Option<String> = sqlx::query_scalar("SELECT login FROM account LIMIT 1")
            .fetch_optional(&self.pool)
            .await?;
        Ok(login.map(|login| Account { login }))
    }

    pub async fn clear_accounts(&self) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM account")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn save_preference(&self, key: &str, value: &str) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO preference (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_preference(&self, key: &str) -> Result<Option<String>, StoreError> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT value FROM preference WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;
        Ok(value)
    }

    pub async fn clear_preference(&self, key: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM preference WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
