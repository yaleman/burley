pub enum BurleyError {
    Io(std::io::Error),
    Other(String),
}

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
