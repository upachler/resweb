use std::{error::Error, fmt::Display};

#[derive(Debug)]
pub struct StringError {
    msg: String,
    source: Option<Box<dyn Error>>,
}

impl StringError {
    pub fn from_source(source: Box<dyn Error>, msg: &str) -> Self {
        Self { source: Some(source), msg: msg.into() }
    }
}

impl Display for StringError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        fmt.write_str(self.msg.as_str())
    }
}
impl Error for StringError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_deref()
    }
}
impl From<String> for StringError 
{
    fn from(msg: String) -> Self {
        StringError {msg, source: None}
    }
}
impl From<&str> for StringError {
    fn from(msg: &str) -> Self {
        StringError {msg: msg.into(), source: None}
    }
}

