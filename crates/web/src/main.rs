use axum::{
    Router,
    extract::{DefaultBodyLimit, Multipart, Query},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
};
use pixa_core::{
    compress::{compress_image, CompressOptions},
    convert::convert_image,
    info::get_image_info,
    prompt::{self, Provider, PromptOptions, PromptLanguage},
    watermark::{WatermarkEngine, WatermarkSize},
};
use serde::Deserialize;
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tempfile::TempDir;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::info;

struct AppState {
    engine: WatermarkEngine,
    upload_dir: TempDir,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let engine = WatermarkEngine::new().expect("Failed to init watermark engine");
    let upload_dir = TempDir::new().expect("Failed to create temp dir");

    info!("Upload dir: {}", upload_dir.path().display());

    let state = Arc::new(AppState { engine, upload_dir });

    // Resolve static file directory:
    //   1. STATIC_DIR env var (explicit override)
    //   2. crates/web/static (cargo run from workspace root)
    //   3. static (Docker / production)
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| {
        if std::path::Path::new("crates/web/static").exists() {
            "crates/web/static".to_string()
        } else {
            "static".to_string()
        }
    });
    info!("Static files: {}", static_dir);

    let app = Router::new()
        // API routes
        .route("/api/info", post(api_info))
        .route("/api/remove-watermark", post(api_remove_watermark))
        .route("/api/detect-watermark", post(api_detect_watermark))
        .route("/api/compress", post(api_compress))
        .route("/api/convert", post(api_convert))
        .route("/api/prompt", post(api_prompt))
        .route("/api/prompt/providers", get(api_prompt_providers))
        .route("/api/health", get(api_health))
        // Static files for SPA
        .fallback_service(ServeDir::new(&static_dir).append_index_html_on_directories(true))
        .layer(CorsLayer::permissive())
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024)) // 50MB
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

type AppResult<T> = Result<T, AppError>;

struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let body = serde_json::json!({ "error": self.0.to_string() });
        (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

/// Save uploaded file to temp dir, return (temp_path, original_filename)
async fn save_upload(
    state: &Arc<AppState>,
    mut multipart: Multipart,
) -> Result<(PathBuf, String), AppError> {
    while let Some(field) = multipart.next_field().await? {
        if field.name() == Some("file") {
            let filename = field
                .file_name()
                .unwrap_or("upload.png")
                .to_string();
            let ext = PathBuf::from(&filename)
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_else(|| "png".to_string());

            let id = uuid::Uuid::new_v4();
            let temp_path = state.upload_dir.path().join(format!("{id}.{ext}"));
            let data = field.bytes().await?;
            tokio::fs::write(&temp_path, &data).await?;

            return Ok((temp_path, filename));
        }
    }
    Err(AppError(anyhow::anyhow!("No file uploaded")))
}

async fn api_health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

async fn api_info(
    state: axum::extract::State<Arc<AppState>>,
    multipart: Multipart,
) -> AppResult<Json<serde_json::Value>> {
    let (path, _) = save_upload(&state, multipart).await?;
    let info = get_image_info(&path)?;
    let _ = tokio::fs::remove_file(&path).await;
    Ok(Json(serde_json::to_value(info)?))
}

#[derive(Deserialize)]
struct WatermarkParams {
    force_size: Option<String>,
}

async fn api_detect_watermark(
    state: axum::extract::State<Arc<AppState>>,
    multipart: Multipart,
) -> AppResult<Json<serde_json::Value>> {
    let (path, _) = save_upload(&state, multipart).await?;
    let img = image::open(&path)?;
    let result = state.engine.detect_watermark(&img, None);
    let _ = tokio::fs::remove_file(&path).await;
    Ok(Json(serde_json::to_value(result)?))
}

async fn api_remove_watermark(
    state: axum::extract::State<Arc<AppState>>,
    Query(params): Query<WatermarkParams>,
    multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    let (path, filename) = save_upload(&state, multipart).await?;
    let mut img = image::open(&path)?;

    let size = params.force_size.map(|s| match s.as_str() {
        "small" => WatermarkSize::Small,
        _ => WatermarkSize::Large,
    });

    state.engine.remove_watermark(&mut img, size)?;

    // Save result
    let out_path = state.upload_dir.path().join(format!("out_{filename}"));
    img.save(&out_path)?;
    let data = tokio::fs::read(&out_path).await?;

    let _ = tokio::fs::remove_file(&path).await;
    let _ = tokio::fs::remove_file(&out_path).await;

    let content_type = match PathBuf::from(&filename)
        .extension()
        .and_then(|e| e.to_str())
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    };

    Ok((
        StatusCode::OK,
        [("content-type", content_type)],
        data,
    ))
}

#[derive(Deserialize)]
struct CompressParams {
    quality: Option<u8>,
    max_width: Option<u32>,
    max_height: Option<u32>,
}

async fn api_compress(
    state: axum::extract::State<Arc<AppState>>,
    Query(params): Query<CompressParams>,
    multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    let (path, filename) = save_upload(&state, multipart).await?;

    let opts = CompressOptions {
        jpeg_quality: params.quality.unwrap_or(80),
        webp_quality: params.quality.unwrap_or(80),
        max_width: params.max_width,
        max_height: params.max_height,
        ..Default::default()
    };

    let out_path = state.upload_dir.path().join(format!("comp_{filename}"));
    let result = compress_image(&path, &out_path, &opts)?;
    let data = tokio::fs::read(&out_path).await?;

    let _ = tokio::fs::remove_file(&path).await;
    let _ = tokio::fs::remove_file(&out_path).await;

    let content_type = match PathBuf::from(&filename)
        .extension()
        .and_then(|e| e.to_str())
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    };

    let result_json = serde_json::to_string(&result).unwrap_or_default();

    Ok((
        StatusCode::OK,
        [
            ("content-type".to_string(), content_type.to_string()),
            ("x-compress-result".to_string(), result_json),
        ],
        data,
    ))
}

#[derive(Deserialize)]
struct ConvertParams {
    format: String,
}

async fn api_convert(
    state: axum::extract::State<Arc<AppState>>,
    Query(params): Query<ConvertParams>,
    multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    let (path, _filename) = save_upload(&state, multipart).await?;

    let out_path = state
        .upload_dir
        .path()
        .join(format!("conv_{}.{}", uuid::Uuid::new_v4(), params.format));
    convert_image(&path, &out_path)?;
    let data = tokio::fs::read(&out_path).await?;

    let _ = tokio::fs::remove_file(&path).await;
    let _ = tokio::fs::remove_file(&out_path).await;

    let content_type = match params.format.as_str() {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        _ => "image/jpeg",
    };

    Ok((StatusCode::OK, [("content-type", content_type)], data))
}

async fn api_prompt_providers() -> Json<serde_json::Value> {
    let available = prompt::detect_available_providers();
    let providers: Vec<_> = available
        .iter()
        .map(|p| serde_json::json!({ "id": p.cli_name(), "name": p.display_name() }))
        .collect();
    Json(serde_json::json!({ "providers": providers }))
}

#[derive(Deserialize)]
struct PromptParams {
    description: Option<String>,
    provider: Option<String>,
    style: Option<String>,
    ratio: Option<String>,
    variations: Option<u8>,
    extra: Option<String>,
}

async fn api_prompt(
    state: axum::extract::State<Arc<AppState>>,
    Query(params): Query<PromptParams>,
    multipart: Option<Multipart>,
) -> AppResult<Json<serde_json::Value>> {
    // Handle optional image upload
    let ref_image_path = if let Some(mut mp) = multipart {
        let mut img_path = None;
        while let Ok(Some(field)) = mp.next_field().await {
            if field.name() == Some("file") {
                let ext = field
                    .file_name()
                    .and_then(|f| PathBuf::from(f).extension().map(|e| e.to_string_lossy().to_string()))
                    .unwrap_or_else(|| "png".to_string());
                let id = uuid::Uuid::new_v4();
                let path = state.upload_dir.path().join(format!("{id}.{ext}"));
                if let Ok(data) = field.bytes().await {
                    if !data.is_empty() {
                        let _ = tokio::fs::write(&path, &data).await;
                        img_path = Some(path);
                    }
                }
                break;
            }
        }
        img_path
    } else {
        None
    };

    // Determine provider
    let provider: Provider = params
        .provider
        .as_deref()
        .unwrap_or("claude")
        .parse()
        .map_err(|e: String| AppError(anyhow::anyhow!(e)))?;

    let opts = PromptOptions {
        description: params.description,
        reference_image: ref_image_path.clone(),
        style: params.style,
        aspect_ratio: params.ratio,
        extra_instructions: params.extra,
        variations: params.variations.unwrap_or(1),
        language: PromptLanguage::English,
    };

    let result = prompt::generate_prompt(provider, &opts)?;

    // Cleanup temp image
    if let Some(path) = ref_image_path {
        let _ = tokio::fs::remove_file(&path).await;
    }

    Ok(Json(serde_json::to_value(result)?))
}
