//! Favorite config files, per user and server.
use chrono::{DateTime, Utc};
use shared::database::Database;
use uuid::Uuid;

const TABLE: &str = "dev_caloptreyx_configeditor_favorites";
/// Favorites a user may keep per server.
pub const MAX_FAVORITES: i64 = 100;

#[derive(sqlx::FromRow)]
pub struct Favorite {
    pub path: String,
    pub created: DateTime<Utc>,
}

impl Favorite {
    /// Ordered by path.
    pub async fn all(db: &Database, user: Uuid, server: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT path, created FROM {TABLE}
             WHERE user_uuid = $1 AND server_uuid = $2
             ORDER BY path"
        )))
        .bind(user)
        .bind(server)
        .fetch_all(db.read())
        .await
    }

    /// Adds the favorite (an existing one is returned as is); `None` when the user already
    /// has [`MAX_FAVORITES`] on the server.
    pub async fn add(
        db: &Database,
        user: Uuid,
        server: Uuid,
        path: &str,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {TABLE} (user_uuid, server_uuid, path)
             SELECT $1, $2, $3
             WHERE (SELECT COUNT(*) FROM {TABLE} WHERE user_uuid = $1 AND server_uuid = $2) < $4
             ON CONFLICT (user_uuid, server_uuid, path) DO NOTHING"
        )))
        .bind(user)
        .bind(server)
        .bind(path)
        .bind(MAX_FAVORITES)
        .execute(db.write())
        .await?;

        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT path, created FROM {TABLE}
             WHERE user_uuid = $1 AND server_uuid = $2 AND path = $3"
        )))
        .bind(user)
        .bind(server)
        .bind(path)
        .fetch_optional(db.write())
        .await
    }

    pub async fn remove(
        db: &Database,
        user: Uuid,
        server: Uuid,
        path: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {TABLE} WHERE user_uuid = $1 AND server_uuid = $2 AND path = $3"
        )))
        .bind(user)
        .bind(server)
        .bind(path)
        .execute(db.write())
        .await?;
        Ok(())
    }
}
