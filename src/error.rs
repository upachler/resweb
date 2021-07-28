use std::{error::Error, fmt::Display};

#[derive(Debug)]
pub struct StringError {
    msg: String
}

impl Display for StringError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        fmt.write_str(self.msg.as_str())
    }
}
impl Error for StringError {
}
impl From<String> for StringError 
{
    fn from(msg: String) -> Self {
        StringError {msg}
    }
}
impl From<&str> for StringError {
    fn from(msg: &str) -> Self {
        StringError {msg: msg.into()}
    }
}

