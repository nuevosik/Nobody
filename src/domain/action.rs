#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionRequest {
    pub id: u32,
    pub revision: u64,
    pub key: String,
}
