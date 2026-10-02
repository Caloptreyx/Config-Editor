use crate::{
    files::{normalize, not_found},
    formats::Format,
};
use shared::{State, models::server::Server, response::ApiResponse};
use utoipa_axum::{router::OpenApiRouter, routes};

mod favorites;
mod file;
mod files;

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(files::list::route))
        .routes(routes!(file::get::route))
        .routes(routes!(file::put::route))
        .routes(routes!(file::put_raw::route))
        .routes(routes!(favorites::list::route))
        .routes(routes!(favorites::add::route))
        .routes(routes!(favorites::remove::route))
        .with_state(state.clone())
}

fn invalid_path() -> ApiResponse {
    ApiResponse::error("invalid path")
}

/// The normalized path and format of a config file the user may access: 400 for invalid
/// paths and unsupported file types, 404 for files hidden from the subuser.
fn config_path(server: &mut Server, path: &str) -> Result<(String, Format), ApiResponse> {
    let path = normalize(path).ok_or_else(invalid_path)?;
    if server.is_ignored(&path, false) {
        return Err(not_found("file"));
    }
    let format =
        Format::detect(&path).ok_or_else(|| ApiResponse::error("this file type is not supported"))?;
    Ok((path, format))
}
