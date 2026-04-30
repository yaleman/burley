use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum BurleyError {
    Io(std::io::Error),
    Other(String),
}

impl Display for BurleyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BurleyError::Io(err) => write!(f, "IO Error: {err}"),
            BurleyError::Other(err) => write!(f, "Error: {err}"),
        }
    }
}

impl std::error::Error for BurleyError {}

impl From<std::io::Error> for BurleyError {
    fn from(err: std::io::Error) -> Self {
        BurleyError::Io(err)
    }
}

impl From<tokio::task::JoinError> for BurleyError {
    fn from(err: tokio::task::JoinError) -> Self {
        BurleyError::Other(err.to_string())
    }
}

impl From<rama::error::OpaqueError> for BurleyError {
    fn from(err: rama::error::OpaqueError) -> Self {
        BurleyError::Other(err.to_string())
    }
}

impl From<rama::error::BoxError> for BurleyError {
    fn from(err: rama::error::BoxError) -> Self {
        BurleyError::Other(err.to_string())
    }
}
