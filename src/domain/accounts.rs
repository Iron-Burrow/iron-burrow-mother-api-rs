#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnchainAccount {
    pub network_slug: String,
    pub address: String,
    pub client_ref: Option<String>,
}
