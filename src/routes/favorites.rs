use crate::{db::Favorite, formats::Format};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(ToSchema, Serialize)]
pub struct ApiFavorite {
    path: String,
    format: Option<Format>,
    created: DateTime<Utc>,
}

impl From<Favorite> for ApiFavorite {
    fn from(favorite: Favorite) -> Self {
        Self {
            format: Format::detect(&favorite.path),
            path: favorite.path,
            created: favorite.created,
        }
    }
}

#[derive(ToSchema, Deserialize)]
pub struct Payload {
    path: String,
}

pub mod list {
    use super::ApiFavorite;
    use crate::db::Favorite;
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::{
            server::GetServer,
            user::{GetPermissionManager, GetUser},
        },
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        favorites: Vec<ApiFavorite>,
    }

    /// The user's favorite config files on this server, by path.
    #[utoipa::path(get, path = "/favorites", responses(
        (status = OK, body = inline(Response)),
        (status = UNAUTHORIZED, body = ApiError),
    ), params(("server" = uuid::Uuid, description = "The server ID")))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
        server: GetServer,
    ) -> ApiResponseResult {
        permissions.has_server_permission("files.read")?;

        let favorites = Favorite::all(&state.database, user.uuid, server.uuid).await?;

        ApiResponse::new_serialized(Response {
            favorites: favorites.into_iter().map(ApiFavorite::from).collect(),
        })
        .ok()
    }
}

pub mod add {
    use super::{ApiFavorite, Payload};
    use crate::db::{Favorite, MAX_FAVORITES};
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::{
            server::GetServer,
            user::{GetPermissionManager, GetUser},
        },
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        favorite: ApiFavorite,
    }

    /// Marks a config file as favorite; adding an existing favorite changes nothing.
    #[utoipa::path(post, path = "/favorites", responses(
        (status = OK, body = inline(Response)),
        (status = BAD_REQUEST, body = ApiError),
        (status = UNAUTHORIZED, body = ApiError),
        (status = NOT_FOUND, body = ApiError),
    ), params(("server" = uuid::Uuid, description = "The server ID")), request_body = inline(Payload))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
        mut server: GetServer,
        shared::Payload(data): shared::Payload<Payload>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("files.read")?;

        let (path, _) = super::super::config_path(&mut server, &data.path)?;
        let favorite = Favorite::add(&state.database, user.uuid, server.uuid, &path)
            .await?
            .ok_or_else(|| {
                ApiResponse::error(format!(
                    "you can keep at most {MAX_FAVORITES} favorites per server"
                ))
            })?;

        ApiResponse::new_serialized(Response {
            favorite: favorite.into(),
        })
        .ok()
    }
}

pub mod remove {
    use super::Payload;
    use crate::{db::Favorite, files::normalize};
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::{
            server::GetServer,
            user::{GetPermissionManager, GetUser},
        },
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {}

    /// Removes a favorite; removing a missing one changes nothing.
    #[utoipa::path(delete, path = "/favorites", responses(
        (status = OK, body = inline(Response)),
        (status = BAD_REQUEST, body = ApiError),
        (status = UNAUTHORIZED, body = ApiError),
    ), params(("server" = uuid::Uuid, description = "The server ID")), request_body = inline(Payload))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
        server: GetServer,
        shared::Payload(data): shared::Payload<Payload>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("files.read")?;

        let path = normalize(&data.path).ok_or_else(super::super::invalid_path)?;
        Favorite::remove(&state.database, user.uuid, server.uuid, &path).await?;

        ApiResponse::new_serialized(Response {}).ok()
    }
}
