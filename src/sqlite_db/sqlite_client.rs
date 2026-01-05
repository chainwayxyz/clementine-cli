use std::borrow::Cow;
use std::path::PathBuf;

use eyre::Context;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};

use crate::errors::BridgeCliError;
use crate::get_clementine_home_dir;

pub fn sqlite_db_path() -> Result<PathBuf, BridgeCliError> {
    let home = get_clementine_home_dir()?;
    Ok(home.join("clementine-cli.db"))
}

#[derive(Clone)]
pub struct SqliteDb {
    pool: Pool<Sqlite>,
}

static MIGRATOR: Migrator = sqlx::migrate!();

impl SqliteDb {
    pub async fn open() -> Result<Self, BridgeCliError> {
        let path = sqlite_db_path()?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).wrap_err("Failed to create DB directory")?;
        }

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .wrap_err("Failed to open SQLite database with sqlx")?;

        Ok(Self { pool })
    }

    pub async fn open_with_schema() -> Result<Self, BridgeCliError> {
        let db = Self::open().await?;

        MIGRATOR.run(db.pool()).await.map_err(|e| {
            tracing::error!("Failed to run database migrations: {}", e);
            BridgeCliError::Eyre(eyre::eyre!("Failed to run database migrations"))
        })?;

        Ok(db)
    }

    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }
}

/// Normalize optional SQLite clients into a usable reference, creating one when none is provided.
pub async fn resolve_sqlite_client<'a>(
    sqlite_client: Option<&'a SqliteDb>,
) -> Result<Cow<'a, SqliteDb>, BridgeCliError> {
    match sqlite_client {
        Some(client) => Ok(Cow::Borrowed(client)),
        None => Ok(Cow::Owned(SqliteDb::open_with_schema().await?)),
    }
}
