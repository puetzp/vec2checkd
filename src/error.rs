use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub(crate) struct MissingFieldError {
    pub field: String,
}

impl fmt::Display for MissingFieldError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "mandatory configuration attribute '{}' is missing",
            self.field
        )
    }
}

impl Error for MissingFieldError {}

#[derive(Debug)]
pub(crate) struct ParseFieldError<'a> {
    pub field: String,
    pub kind: &'a str,
}

impl<'a> fmt::Display for ParseFieldError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "failed to parse configuration attribute '{}' as {}",
            self.field, self.kind
        )
    }
}

impl<'a> Error for ParseFieldError<'a> {}
