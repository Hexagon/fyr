/// Representation of MimeTypes.
#[derive(Debug, PartialEq, Eq)]
pub enum MimeType {
    /// A special "MimeType" that represents a redirection
    Redirect,
    LinkTarget,
    DeletedEntry,
    Type(String),
}
