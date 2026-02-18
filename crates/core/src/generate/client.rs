use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use reqwest::Client;
use serde_json::{json, Value};
use tracing::{debug, warn};

use super::error::GenerateError;
use super::types::GeminiConfig;

/// Raw response from a single Gemini API call
pub struct ClientResponse {
    /// Decoded image bytes (if image was returned)
    pub image_data: Option<Vec<u8>>,
    /// Text response (if any)
    pub text_response: Option<String>,
}

/// Low-level Gemini API HTTP client
pub struct GeminiClient {
    http: Client,
    config: GeminiConfig,
}

impl GeminiClient {
    pub fn new(config: GeminiConfig) -> Self {
        let http = Client::new();
        Self { http, config }
    }

    /// Get the current model ID
    pub fn model_id(&self) -> &str {
        self.config.model.model_id()
    }

    /// Build the API endpoint URL
    fn endpoint_url(&self) -> String {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.config.model.model_id(),
            self.config.api_key
        )
    }

    /// Call generateContent with text-only prompt
    /// Matches nanobanana imageGenerator.ts:260-268
    pub async fn generate_from_text(
        &self,
        prompt: &str,
    ) -> Result<ClientResponse, GenerateError> {
        let body = json!({
            "contents": [{
                "role": "user",
                "parts": [{ "text": prompt }]
            }]
        });
        self.call_api(&body).await
    }

    /// Call generateContent with text + inline image (for edit/restore)
    /// Matches nanobanana imageGenerator.ts:569-585
    pub async fn generate_with_image(
        &self,
        prompt: &str,
        image_bytes: &[u8],
        mime_type: &str,
    ) -> Result<ClientResponse, GenerateError> {
        let image_base64 = BASE64.encode(image_bytes);

        let body = json!({
            "contents": [{
                "role": "user",
                "parts": [
                    { "text": prompt },
                    {
                        "inlineData": {
                            "mimeType": mime_type,
                            "data": image_base64
                        }
                    }
                ]
            }]
        });
        self.call_api(&body).await
    }

    /// Execute the API call and parse the response
    async fn call_api(&self, body: &Value) -> Result<ClientResponse, GenerateError> {
        debug!("Calling Gemini API: model={}", self.config.model.model_id());

        let response = self
            .http
            .post(&self.endpoint_url())
            .json(body)
            .timeout(std::time::Duration::from_secs(self.config.timeout_secs))
            .send()
            .await?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(Self::map_api_error(status, &error_body));
        }

        let json: Value = response.json().await?;
        Self::parse_response(&json)
    }

    /// Parse API response, extracting image data from candidates
    /// Matches nanobanana imageGenerator.ts:272-308
    fn parse_response(json: &Value) -> Result<ClientResponse, GenerateError> {
        let parts = json["candidates"][0]["content"]["parts"]
            .as_array()
            .ok_or(GenerateError::NoImageInResponse)?;

        let mut image_data: Option<Vec<u8>> = None;
        let mut text_response: Option<String> = None;

        for part in parts {
            // Primary: inlineData.data
            if let Some(inline) = part.get("inlineData") {
                if let Some(data) = inline["data"].as_str() {
                    match BASE64.decode(data) {
                        Ok(bytes) if bytes.len() > 100 => {
                            debug!("Found image data in inlineData: {} bytes", bytes.len());
                            image_data = Some(bytes);
                            break;
                        }
                        Ok(_) => {
                            debug!("Skipping short inlineData");
                        }
                        Err(e) => {
                            warn!("Failed to decode inlineData base64: {e}");
                        }
                    }
                }
            }

            // Fallback: text field containing base64 image data
            // Matches nanobanana imageGenerator.ts:136-159
            if let Some(text) = part["text"].as_str() {
                if Self::is_valid_base64_image_data(text) {
                    match BASE64.decode(text) {
                        Ok(bytes) => {
                            debug!("Found image data in text field (fallback)");
                            image_data = Some(bytes);
                            break;
                        }
                        Err(e) => {
                            warn!("Failed to decode text base64: {e}");
                        }
                    }
                } else {
                    text_response = Some(text.to_string());
                }
            }
        }

        Ok(ClientResponse {
            image_data,
            text_response,
        })
    }

    /// Check if a string looks like valid base64 image data
    /// Matches nanobanana imageGenerator.ts:136-159
    fn is_valid_base64_image_data(data: &str) -> bool {
        if data.len() < 1000 {
            return false;
        }
        // Check if it's valid base64 format
        data.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
    }

    /// Map HTTP error codes to GenerateError
    /// Matches nanobanana imageGenerator.ts:356-400
    fn map_api_error(status: u16, body: &str) -> GenerateError {
        let lower = body.to_lowercase();

        if lower.contains("api key not valid") {
            return GenerateError::AuthFailed(
                "The provided API key is invalid. Please check your PIXA_GEMINI_API_KEY environment variable.".into(),
            );
        }

        if lower.contains("permission denied") {
            return GenerateError::AuthFailed(
                "The provided API key does not have the necessary permissions for the Gemini API.".into(),
            );
        }

        if lower.contains("quota exceeded") {
            return GenerateError::QuotaExceeded;
        }

        match status {
            400 if lower.contains("safety") => GenerateError::SafetyBlocked,
            400 => GenerateError::Api {
                status,
                message: "The request was malformed. This may be due to an issue with the prompt. Please check for safety violations or unsupported content.".into(),
            },
            401 | 403 => GenerateError::AuthFailed(format!(
                "Authentication failed (HTTP {status}). Please ensure your API key is valid and has the necessary permissions."
            )),
            429 => GenerateError::RateLimited,
            500..=599 => GenerateError::Api {
                status,
                message: "The image generation service encountered a temporary internal error. Please try again later.".into(),
            },
            _ => GenerateError::Api {
                status,
                message: format!("API request failed with status {status}. Please check your connection and API key."),
            },
        }
    }

    /// Detect MIME type from file extension
    pub fn detect_mime_type(path: &std::path::Path) -> &'static str {
        match path.extension().and_then(|e| e.to_str()) {
            Some("png") => "image/png",
            Some("webp") => "image/webp",
            Some("gif") => "image/gif",
            _ => "image/jpeg",
        }
    }
}
