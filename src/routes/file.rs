use crate::{
    files::Files,
    formats::{Format, Node, ParseError},
};
use axum::http::StatusCode;
use serde::Serialize;
use sha2::Digest;
use shared::{GetState, models::server::ServerActivityLogger, response::ApiResponse};
use utoipa::ToSchema;

#[derive(ToSchema, Serialize)]
pub struct FileResponse {
    path: String,
    format: Format,
    /// sha256 of the raw bytes, hex.
    hash: String,
    content: String,
    /// `None` when the file does not parse; `error` says why.
    document: Option<Node>,
    error: Option<ParseError>,
}

impl FileResponse {
    fn new(path: String, format: Format, content: String) -> Self {
        let (document, error) = match format.parse(&content) {
            Ok(document) => (Some(document), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            path,
            format,
            hash: hash(&content),
            content,
            document,
            error,
        }
    }
}

#[derive(ToSchema, Serialize)]
pub struct WriteResponse {
    #[serde(flatten)]
    file: FileResponse,
    changed: bool,
}

fn hash(content: &str) -> String {
    hex::encode(sha2::Sha256::digest(content.as_bytes()))
}

async fn max_size(state: &GetState) -> Result<u64, ApiResponse> {
    Ok(state
        .settings
        .get_as(|settings| settings.server.max_file_manager_view_size)
        .await?)
}

/// The file as text: 422 when it is not UTF-8.
async fn read_text(files: &Files<'_>, path: &str, max_size: u64) -> Result<String, ApiResponse> {
    String::from_utf8(files.read(path, max_size).await?).map_err(|_| {
        ApiResponse::error("the file is not UTF-8 text")
            .with_status(StatusCode::UNPROCESSABLE_ENTITY)
    })
}

/// Shared by both writes: re-reads the file, checks it is still the version the user
/// opened (unless `force`), and writes `edit(current)` when it differs.
struct Write<'a> {
    state: &'a GetState,
    files: Files<'a>,
    activity_logger: &'a ServerActivityLogger,
    user: uuid::Uuid,
    path: String,
    format: Format,
}

impl Write<'_> {
    async fn run(
        self,
        expected_hash: &str,
        force: bool,
        dry_run: bool,
        edit: impl FnOnce(&str) -> Result<String, Vec<String>>,
    ) -> Result<WriteResponse, ApiResponse> {
        let max_size = max_size(self.state).await?;
        let current = read_text(&self.files, &self.path, max_size).await?;
        if !force && hash(&current) != expected_hash {
            return Err(ApiResponse::error("the file changed since it was opened")
                .with_status(StatusCode::CONFLICT));
        }

        let content = edit(&current).map_err(ApiResponse::errors)?;
        if content.len() as u64 > max_size {
            return Err(ApiResponse::error("the new content is too large to save")
                .with_status(StatusCode::PAYLOAD_TOO_LARGE));
        }
        let changed = content != current;
        if changed && !dry_run {
            let revision_id = self
                .files
                .write(&self.path, self.user, content.clone().into_bytes())
                .await?;
            self.activity_logger
                .log(
                    "server:file.write",
                    serde_json::json!({ "file": self.path, "revision_id": revision_id }),
                )
                .await;
        }

        Ok(WriteResponse {
            file: FileResponse::new(self.path, self.format, content),
            changed,
        })
    }
}

pub mod get {
    use super::{FileResponse, max_size, read_text};
    use crate::files::Files;
    use serde::Deserialize;
    use shared::{
        ApiError, GetState,
        models::{server::GetServer, user::GetPermissionManager},
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Deserialize)]
    pub struct Params {
        path: String,
    }

    /// The file's text and, when it parses, its document.
    #[utoipa::path(get, path = "/file", responses(
        (status = OK, body = inline(FileResponse)),
        (status = BAD_REQUEST, body = ApiError),
        (status = UNAUTHORIZED, body = ApiError),
        (status = NOT_FOUND, body = ApiError),
        (status = PAYLOAD_TOO_LARGE, body = ApiError),
        (status = UNPROCESSABLE_ENTITY, body = ApiError),
    ), params(
        ("server" = uuid::Uuid, description = "The server ID"),
        ("path" = String, Query, description = "The config file", example = "/server.properties"),
    ))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        mut server: GetServer,
        axum::extract::Query(params): axum::extract::Query<Params>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("files.read-content")?;

        let (path, format) = super::super::config_path(&mut server, &params.path)?;
        let max_size = max_size(&state).await?;
        let files = Files::connect(&state, &server).await?;
        let content = read_text(&files, &path, max_size).await?;

        ApiResponse::new_serialized(FileResponse::new(path, format, content)).ok()
    }
}

pub mod put {
    use super::{Write, WriteResponse};
    use crate::{files::Files, formats::Node};
    use serde::Deserialize;
    use shared::{
        ApiError, GetState,
        models::{
            server::{GetServer, GetServerActivityLogger},
            user::{GetPermissionManager, GetUser},
        },
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Deserialize)]
    pub struct Payload {
        path: String,
        /// `hash` of the version the document was made from.
        expected_hash: String,
        document: Node,
        /// Only compute the result.
        #[serde(default)]
        dry_run: bool,
        /// Save even when the file changed since it was opened.
        #[serde(default)]
        force: bool,
    }

    /// Applies the document to the file, keeping everything that did not change.
    #[utoipa::path(put, path = "/file", responses(
        (status = OK, body = inline(WriteResponse)),
        (status = BAD_REQUEST, body = ApiError),
        (status = UNAUTHORIZED, body = ApiError),
        (status = NOT_FOUND, body = ApiError),
        (status = CONFLICT, body = ApiError),
        (status = PAYLOAD_TOO_LARGE, body = ApiError),
        (status = UNPROCESSABLE_ENTITY, body = ApiError),
    ), params(("server" = uuid::Uuid, description = "The server ID")), request_body = inline(Payload))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
        mut server: GetServer,
        activity_logger: GetServerActivityLogger,
        shared::Payload(data): shared::Payload<Payload>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("files.create")?;

        let (path, format) = super::super::config_path(&mut server, &data.path)?;
        let write = Write {
            state: &state,
            files: Files::connect(&state, &server).await?,
            activity_logger: &activity_logger,
            user: user.uuid,
            path,
            format,
        };
        let response = write
            .run(&data.expected_hash, data.force, data.dry_run, |current| {
                format.apply(current, &data.document)
            })
            .await?;

        ApiResponse::new_serialized(response).ok()
    }
}

pub mod put_raw {
    use super::{Write, WriteResponse};
    use crate::files::Files;
    use serde::Deserialize;
    use shared::{
        ApiError, GetState,
        models::{
            server::{GetServer, GetServerActivityLogger},
            user::{GetPermissionManager, GetUser},
        },
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Deserialize)]
    pub struct Payload {
        path: String,
        /// `hash` of the version the content was made from.
        expected_hash: String,
        content: String,
        /// Save even when the file changed since it was opened.
        #[serde(default)]
        force: bool,
    }

    /// Replaces the file's text.
    #[utoipa::path(put, path = "/file/raw", responses(
        (status = OK, body = inline(WriteResponse)),
        (status = BAD_REQUEST, body = ApiError),
        (status = UNAUTHORIZED, body = ApiError),
        (status = NOT_FOUND, body = ApiError),
        (status = CONFLICT, body = ApiError),
        (status = PAYLOAD_TOO_LARGE, body = ApiError),
        (status = UNPROCESSABLE_ENTITY, body = ApiError),
    ), params(("server" = uuid::Uuid, description = "The server ID")), request_body = inline(Payload))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
        mut server: GetServer,
        activity_logger: GetServerActivityLogger,
        shared::Payload(data): shared::Payload<Payload>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("files.create")?;

        let (path, format) = super::super::config_path(&mut server, &data.path)?;
        let write = Write {
            state: &state,
            files: Files::connect(&state, &server).await?,
            activity_logger: &activity_logger,
            user: user.uuid,
            path,
            format,
        };
        let content = data.content;
        let response = write
            .run(&data.expected_hash, data.force, false, |_| Ok(content))
            .await?;

        ApiResponse::new_serialized(response).ok()
    }
}
