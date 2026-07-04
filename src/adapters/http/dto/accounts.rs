use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OnchainAccountRequest {
    pub(crate) network_slug: String,
    pub(crate) address: String,
    pub(crate) client_ref: Option<String>,
}
