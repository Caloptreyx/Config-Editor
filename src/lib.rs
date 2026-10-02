use shared::{
    State,
    extensions::{Extension, ExtensionRouteBuilder},
};

mod db;
mod files;
mod formats;
mod routes;

#[derive(Default)]
pub struct ExtensionStruct;

#[async_trait::async_trait]
impl Extension for ExtensionStruct {
    async fn initialize_router(
        &mut self,
        state: State,
        builder: ExtensionRouteBuilder,
    ) -> ExtensionRouteBuilder {
        builder.add_client_server_api_router(|router| {
            router.nest("/config-editor", routes::router(&state))
        })
    }
}
