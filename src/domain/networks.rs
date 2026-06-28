#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NetworkRef {
    pub(crate) slug: String,
    pub(crate) name: String,
    pub(crate) caip2: Option<String>,
    pub(crate) family: String,
    pub(crate) chain_id: Option<i64>,
}
