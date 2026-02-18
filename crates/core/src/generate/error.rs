use thiserror::Error;

#[derive(Error, Debug)]
pub enum GenerateError {
    #[error("API key not configured. Set PIXA_GEMINI_API_KEY, GEMINI_API_KEY, or GOOGLE_API_KEY environment variable.")]
    ApiKeyNotFound,

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Rate limit exceeded. Please wait and try again.")]
    RateLimited,

    #[error("API quota exceeded. Check your Google Cloud billing.")]
    QuotaExceeded,

    #[error("Safety filter blocked the request. Try rephrasing your prompt.")]
    SafetyBlocked,

    #[error("No image data in API response")]
    NoImageInResponse,

    #[error("Invalid input image: {0}")]
    InvalidInputImage(String),

    #[error("Image file not found: {0}")]
    ImageNotFound(String),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Base64 decode error: {0}")]
    Base64Decode(String),
}
