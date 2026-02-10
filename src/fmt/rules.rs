/// Formatting rules applied during AST traversal.

/// Represents the kind of whitespace to insert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spacing {
    /// No space.
    None,
    /// A single space.
    Space,
    /// A newline.
    Newline,
    /// Two newlines (blank line).
    BlankLine,
}
