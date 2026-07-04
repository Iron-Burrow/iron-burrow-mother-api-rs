use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TokenSelectorRequest {
    #[serde(default)]
    pub(crate) asset_slugs: Vec<String>,
    #[serde(default)]
    pub(crate) contract_addresses: Vec<String>,
}
