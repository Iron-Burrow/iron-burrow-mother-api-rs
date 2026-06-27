#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkRef {
    pub slug: String,
    pub name: String,
    pub caip2: Option<String>,
    pub family: String,
    pub chain_id: Option<i64>,
}
