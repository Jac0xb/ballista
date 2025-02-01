pub mod bankclient;
pub mod trait_definition;

pub use super::Result;
pub use bankclient::*;
pub use trait_definition::*;

pub async fn create_test_context() -> Result<Box<dyn TestContext>> {
    new_bankclient_test_context().await
}
