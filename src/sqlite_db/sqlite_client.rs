use std::path::PathBuf;

use eyre::Context;
use sqlx::sqlite::SqlitePoolOptions;
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

impl SqliteDb {
    pub async fn open() -> Result<Self, BridgeCliError> {
        let path = sqlite_db_path()?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).wrap_err("Failed to create DB directory")?;
        }

        let url = format!("sqlite:{}", path.display());

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .wrap_err("Failed to open SQLite database with sqlx")?;

        Ok(Self { pool })
    }

    pub async fn open_with_schema() -> Result<Self, BridgeCliError> {
        let db = Self::open().await?;

        crate::sqlite_db::wallet_db::WalletTable::ensure_exists(db.pool()).await?;

        Ok(db)
    }

    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }
}

pub trait SqliteTable {
    const TABLE_NAME: &'static str;

    const CREATE_SQL: &'static str;

    async fn ensure_exists(pool: &Pool<Sqlite>) -> Result<(), BridgeCliError> {
        sqlx::query(Self::CREATE_SQL)
            .execute(pool)
            .await
            .wrap_err_with(|| format!("Failed to create table {}", Self::TABLE_NAME))?;

        Ok(())
    }
}
