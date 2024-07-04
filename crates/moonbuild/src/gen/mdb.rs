#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiAlias {
    pub name: String,
    pub alias: String,
}

impl PartialOrd for MiAlias {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.name.cmp(&other.name))
    }
}

impl Ord for MiAlias {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}
