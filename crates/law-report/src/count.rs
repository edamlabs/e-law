use std::fmt;

/// A number of things, its noun in the singular or the plural as the number asks.
pub(crate) struct Count(pub(crate) usize, pub(crate) &'static str);

impl fmt::Display for Count {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plural = if self.0 == 1 { "" } else { "s" };
        write!(f, "{} {}{plural}", self.0, self.1)
    }
}
