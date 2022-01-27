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
pub(crate) struct InvalidPluginOutputError<'a> {
    pub mapping_name: String,
    pub reference: &'a str,
}

impl<'a> fmt::Display for InvalidPluginOutputError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "configuration parameter '{}.plugin_output' is invalid as it references the non-existent parameter '{}.{}' using the placeholder '${}'",
            self.mapping_name, self.mapping_name, self.reference, self.reference
        )
    }
}

impl<'a> Error for InvalidPluginOutputError<'a> {}

#[derive(Debug)]
pub(crate) struct MissingLabelError {
    pub identifier: String,
    pub label: String,
}

impl<'a> fmt::Display for MissingLabelError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "failed to replace plugin output placeholder '{}' with value of label '{}'",
            self.identifier, self.label
        )
    }
}

impl Error for MissingLabelError {}
