// Taken from https://www.reddit.com/r/rust/comments/p74zza/comment/h9iervn/?utm_source=share&utm_medium=web2x&context=3

/// Allows creating multiple versions of the same struct with fields wrapped
/// in a different type.
pub trait Wrap {
    type Wrapped<T>;
}

/// Does not wrap fields in any type.
#[derive(Clone, Default)]
pub struct RequiredFields;
impl Wrap for RequiredFields {
    type Wrapped<T> = T;
}

/// Wraps all fields in an `Option`.
#[derive(Clone, Default)]
pub struct OptionalFields;
impl Wrap for OptionalFields {
    type Wrapped<T> = Option<T>;
}
