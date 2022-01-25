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

#[derive(Debug)]
pub(crate) struct MissingLabelError<'a> {
    pub identifier: String,
    pub label: &'a str,
}

impl<'a> fmt::Display for MissingLabelError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "failed to replace plugin output identifier '{}' with value of label '{}'",
            self.identifier, self.label
        )
    }
}

impl<'a> Error for MissingLabelError<'a> {}

#[derive(Debug)]
pub(crate) struct MissingThresholdError<'a> {
    pub identifier: &'a str,
    pub threshold: &'a str,
}

impl<'a> fmt::Display for MissingThresholdError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "failed to replace plugin output identifier '{}' with value of {} threshold",
            self.identifier, self.threshold
        )
    }
}

impl<'a> Error for MissingThresholdError<'a> {}
