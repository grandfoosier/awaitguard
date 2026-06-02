#[derive(Debug, Clone)]
pub struct ChangedFile {
    pub path: String,
    pub patch: Option<String>,
    pub is_rust: bool,
}
