use std::{
    error::Error,
    fmt::{self, Display},
};

#[derive(Debug)]
pub struct AnyError(pub Box<dyn Error>);

impl AnyError {
    pub fn new<E: Error + 'static>(error: E) -> Self {
        AnyError(Box::new(error))
    }
}

impl Error for AnyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.as_ref())
    }
}

impl Display for AnyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}
