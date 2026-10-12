#[derive(Debug, thiserror::Error)]
pub enum RsConfigError {
    #[error("{name} is missing")]
    Missing { name: String },
    #[error("{name} must not be empty")]
    Empty { name: String },
    #[error("{name} is not valid UTF-8")]
    NotUnicode { name: String },
    #[error("{name} is invalid: {message}")]
    Invalid {
        name: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },
}

impl RsConfigError {
    pub fn missing(name: impl Into<String>) -> Self {
        Self::Missing { name: name.into() }
    }

    pub fn empty(name: impl Into<String>) -> Self {
        Self::Empty { name: name.into() }
    }

    pub fn not_unicode(name: impl Into<String>) -> Self {
        Self::NotUnicode { name: name.into() }
    }

    pub fn invalid(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Invalid {
            name: name.into(),
            message: message.into(),
            source: None,
        }
    }

    pub fn invalid_source(
        name: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Invalid {
            name: name.into(),
            message: source.to_string(),
            source: Some(Box::new(source)),
        }
    }
}
