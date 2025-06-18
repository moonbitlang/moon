/// The context that encapsulates all the data needed for the building process.
pub struct BuildContext {}

/// The high-level intent of the user.
#[derive(Clone, Debug)]
pub enum UserIntent {
    Check,
    Build,
    Run,
    Test,
    Format,
    Info,
    Bundle,
}
