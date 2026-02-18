pub mod client;
pub mod config;
pub mod error;
pub mod operations;
pub mod prompt_builder;
pub mod types;

// Convenience re-exports
pub use client::GeminiClient;
pub use error::GenerateError;
pub use operations::*;
pub use types::*;
