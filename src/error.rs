use std::{error::Error, fmt};

#[derive(Debug)]
pub enum CliError {
    MissingApiKey,
    InvalidBaseUrl(String),
    InvalidInput {
        message: String,
        hint: Option<String>,
    },
    Transport {
        message: String,
        hint: Option<String>,
    },
    Api {
        status: u16,
        message: String,
        hint: Option<String>,
    },
    ResponseParse {
        message: String,
        hint: Option<String>,
    },
    Output(String),
}

impl CliError {
    pub fn hint(&self) -> Option<&str> {
        match self {
            CliError::MissingApiKey => Some(
                "Set RECLAIM_API_KEY or pass --api-key. You can find your key in Reclaim settings.",
            ),
            CliError::InvalidBaseUrl(_) => {
                Some("Use a valid URL, e.g. --base-url https://api.app.reclaim.ai/api")
            }
            CliError::InvalidInput { hint, .. }
            | CliError::Transport { hint, .. }
            | CliError::Api { hint, .. }
            | CliError::ResponseParse { hint, .. } => hint.as_deref(),
            CliError::Output(_) => None,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::MissingApiKey => write!(f, "Missing Reclaim API key."),
            CliError::InvalidBaseUrl(url) => write!(f, "Invalid base URL: {url}"),
            CliError::InvalidInput { message, .. } => write!(f, "{message}"),
            CliError::Transport { message, .. } => write!(f, "{message}"),
            CliError::Api {
                status, message, ..
            } => write!(f, "Reclaim API returned HTTP {status}: {message}"),
            CliError::ResponseParse { message, .. } => write!(f, "{message}"),
            CliError::Output(message) => write!(f, "{message}"),
        }
    }
}

impl Error for CliError {}
