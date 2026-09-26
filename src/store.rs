use std::path::Path;

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use thiserror::Error;

use crate::github::Account;
use crate::release::{BuildDispatch, ChannelPointer};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database failure: {0}")]
    Database(#[from] sqlx::Error),
}

/// Local SQLite storage. Holds account metadata and preferences only —
/// tokens live in the OS keyring, and release definitions live in the repo
/// manifest (ADR-0003).
const PROXY_PREFERENCE: &str = "network.proxy";

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
        // 通道指针：一个 (仓库, 发布目标, 通道) 指向一个发布版本（ADR-0005）。
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS channel_pointer (
                repository TEXT NOT NULL,
                target TEXT NOT NULL,
                channel TEXT NOT NULL,
                version TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (repository, target, channel)
            )",
        )
        .execute(&self.pool)
        .await?;
        // 控制台自己触发的构建：dispatch 的返回值里没有 run id，所以在 run 列表里
        // 认领到之后再补上（见 ReleaseBoard::bind_dispatches）。
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS build_dispatch (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                repository TEXT NOT NULL,
                target TEXT NOT NULL,
                version TEXT NOT NULL,
                config TEXT NOT NULL,
                dispatched_at TEXT NOT NULL,
                run_id INTEGER
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

    /// The proxy every GitHub request should go through, if one is set.
    pub async fn save_proxy(&self, proxy: Option<&str>) -> Result<(), StoreError> {
        match proxy.map(str::trim).filter(|value| !value.is_empty()) {
            Some(value) => self.save_preference(PROXY_PREFERENCE, value).await,
            None => self.clear_preference(PROXY_PREFERENCE).await,
        }
    }

    pub async fn load_proxy(&self) -> Result<Option<String>, StoreError> {
        self.load_preference(PROXY_PREFERENCE).await
    }

    pub async fn clear_preference(&self, key: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM preference WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Point one channel at one release version for one release target.
    pub async fn save_channel_pointer(&self, pointer: &ChannelPointer) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO channel_pointer (repository, target, channel, version, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(repository, target, channel) DO UPDATE SET
                version = excluded.version,
                updated_at = excluded.updated_at",
        )
        .bind(&pointer.repository)
        .bind(&pointer.target)
        .bind(&pointer.channel)
        .bind(&pointer.version)
        .bind(&pointer.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Every pointer a repository has, ordered target then channel so the board
    /// can lay them out the same way every time.
    pub async fn load_channel_pointers(
        &self,
        repository: &str,
    ) -> Result<Vec<ChannelPointer>, StoreError> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
            "SELECT repository, target, channel, version, updated_at
             FROM channel_pointer
             WHERE repository = ?
             ORDER BY target, channel",
        )
        .bind(repository)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(repository, target, channel, version, updated_at)| ChannelPointer {
                    repository,
                    target,
                    channel,
                    version,
                    updated_at,
                },
            )
            .collect())
    }

    /// Record a build the console asked for, and return it with its row id.
    pub async fn record_dispatch(&self, dispatch: &BuildDispatch) -> Result<i64, StoreError> {
        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO build_dispatch (repository, target, version, config, dispatched_at, run_id)
             VALUES (?, ?, ?, ?, ?, ?)
             RETURNING id",
        )
            .bind(&dispatch.repository)
            .bind(&dispatch.target)
            .bind(&dispatch.version)
            .bind(&dispatch.config)
            .bind(&dispatch.dispatched_at)
            .bind(dispatch.run_id.map(|id| id as i64))
            .fetch_one(&self.pool)
            .await?;
        Ok(id)
    }

    /// The dispatches a repository has, oldest first.
    pub async fn load_dispatches(
        &self,
        repository: &str,
    ) -> Result<Vec<BuildDispatch>, StoreError> {
        let rows = sqlx::query_as::<_, (i64, String, String, String, String, String, Option<i64>)>(
            "SELECT id, repository, target, version, config, dispatched_at, run_id
             FROM build_dispatch
             WHERE repository = ?
             ORDER BY id",
        )
        .bind(repository)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(
                |(id, repository, target, version, config, dispatched_at, run_id)| BuildDispatch {
                    id,
                    repository,
                    target,
                    version,
                    config,
                    dispatched_at,
                    run_id: run_id.map(|id| id as u64),
                },
            )
            .collect())
    }

    /// Attach a run id to a dispatch the run list just proved.
    pub async fn bind_dispatch(&self, id: i64, run_id: u64) -> Result<(), StoreError> {
        sqlx::query("UPDATE build_dispatch SET run_id = ? WHERE id = ?")
            .bind(run_id as i64)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
