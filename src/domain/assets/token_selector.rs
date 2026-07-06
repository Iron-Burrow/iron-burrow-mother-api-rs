#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TokenSelector {
    pub asset_slugs: Vec<String>,
    pub contract_addresses: Vec<String>,
}

impl TokenSelector {
    pub fn len(&self) -> usize {
        self.asset_slugs.len() + self.contract_addresses.len()
    }
}
