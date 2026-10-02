//! Server file access through Wings, with the same checks and error mapping as the panel's
//! own file routes.
use axum::http::StatusCode;
use shared::{ApiError, State, models::server::Server, response::ApiResponse};
use tokio::io::AsyncReadExt;
use wings_api::client::{ApiHttpError, AsyncRequestReader, WingsClient};

const LIST_PER_PAGE: u64 = 100;
const LIST_MAX_PAGES: u64 = 50;

/// `path` as an absolute `/`-separated server path; `None` when it contains NUL or `..`
/// leaves the server root.
pub fn normalize(path: &str) -> Option<String> {
    if path.contains('\0') {
        return None;
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(format!("/{}", parts.join("/")))
}

/// `name` inside `directory` (both normalized).
pub fn join(directory: &str, name: &str) -> String {
    if directory == "/" {
        format!("/{name}")
    } else {
        format!("{directory}/{name}")
    }
}

pub fn not_found(what: &str) -> ApiResponse {
    ApiResponse::error(format!("{what} not found")).with_status(StatusCode::NOT_FOUND)
}

fn wings_status(status: StatusCode, err: wings_api::ApiError) -> ApiResponse {
    ApiResponse::new_serialized(ApiError::new_wings_value(err)).with_status(status)
}

pub struct Files<'a> {
    client: WingsClient,
    server: &'a Server,
}

impl<'a> Files<'a> {
    pub async fn connect(state: &State, server: &'a Server) -> Result<Self, ApiResponse> {
        let client = server
            .node
            .fetch_cached(&state.database)
            .await?
            .api_client(&state.database)
            .await?;
        Ok(Self { client, server })
    }

    fn ignored(&self) -> Option<Vec<compact_str::CompactString>> {
        self.server.subuser_ignored_files.clone()
    }

    /// Every entry of the directory (up to [`LIST_MAX_PAGES`] pages).
    pub async fn list(&self, directory: &str) -> Result<Vec<wings_api::DirectoryEntry>, ApiResponse> {
        let mut query = wings_api::servers_server_files_list::get::Query {
            directory: Some(directory.into()),
            ignored: self.ignored(),
            per_page: Some(LIST_PER_PAGE),
            ..Default::default()
        };
        let mut entries = Vec::new();
        for page in 1..=LIST_MAX_PAGES {
            query.page = Some(page);
            let response = match self
                .client
                .get_servers_server_files_list(self.server.uuid, &query)
                .await
            {
                Ok(response) => response,
                Err(ApiHttpError::Http(StatusCode::NOT_FOUND, err)) => {
                    return Err(wings_status(StatusCode::NOT_FOUND, err));
                }
                Err(err) => return Err(err.into()),
            };
            let received = response.entries.len() as u64;
            entries.extend(response.entries);
            if received < LIST_PER_PAGE || entries.len() as u64 >= response.total {
                break;
            }
        }
        Ok(entries)
    }

    /// The file's bytes, at most `max_size` of them (413 beyond).
    pub async fn read(&self, path: &str, max_size: u64) -> Result<Vec<u8>, ApiResponse> {
        let reader = match self
            .client
            .get_servers_server_files_contents(
                self.server.uuid,
                &wings_api::servers_server_files_contents::get::Query {
                    file: Some(path.into()),
                    max_size: Some(max_size),
                    ignored: self.ignored(),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(reader) => reader,
            Err(ApiHttpError::Http(
                status @ (StatusCode::NOT_FOUND | StatusCode::PAYLOAD_TOO_LARGE),
                err,
            )) => return Err(wings_status(status, err)),
            Err(err) => return Err(err.into()),
        };

        let mut data = Vec::new();
        reader
            .take(max_size.saturating_add(1))
            .read_to_end(&mut data)
            .await?;
        if data.len() as u64 > max_size {
            return Err(ApiResponse::error("file is too large to edit")
                .with_status(StatusCode::PAYLOAD_TOO_LARGE));
        }
        Ok(data)
    }

    /// Writes the file as `user`; the Wings revision id of the new content.
    pub async fn write(
        &self,
        path: &str,
        user: uuid::Uuid,
        data: Vec<u8>,
    ) -> Result<Option<i64>, ApiResponse> {
        match self
            .client
            .post_servers_server_files_write(
                self.server.uuid,
                AsyncRequestReader::new(std::io::Cursor::new(data)),
                &wings_api::servers_server_files_write::post::Query {
                    file: Some(path.into()),
                    user: Some(user),
                    ignored: self.ignored(),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(response) => Ok(response.revision_id),
            Err(ApiHttpError::Http(
                status @ (StatusCode::NOT_FOUND | StatusCode::EXPECTATION_FAILED),
                err,
            )) => Err(wings_status(status, err)),
            Err(err) => Err(err.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_paths_inside_the_root() {
        assert_eq!(normalize("server.properties").as_deref(), Some("/server.properties"));
        assert_eq!(normalize("//plugins/./x/../y.yml").as_deref(), Some("/plugins/y.yml"));
        assert_eq!(normalize("/").as_deref(), Some("/"));
        assert_eq!(normalize("/a/../..").as_deref(), None);
        assert_eq!(normalize("/../etc/passwd").as_deref(), None);
        assert_eq!(normalize("/a\0b").as_deref(), None);
    }

    #[test]
    fn joins_into_directories() {
        assert_eq!(join("/", "a.yml"), "/a.yml");
        assert_eq!(join("/plugins", "a.yml"), "/plugins/a.yml");
    }
}
