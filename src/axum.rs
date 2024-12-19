use std::io::Write;

use anyhow::{Context, Result};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::{fs::File, io::AsyncReadExt, net::TcpListener, signal};
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::{self, TraceLayer},
};
use tracing::Level;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};
use utoipa::{OpenApi, ToSchema};

use crate::app_error_v2;

#[derive(Debug)]
#[deprecated = "Use app_error_v2::AppError for a more flexible app error interface"]
pub struct AppError(pub anyhow::Error);

// Tell axum how to convert `AppError` into a response.
#[allow(deprecated)]
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}
// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, AppError>`. That way you don't need to do that manually.
#[allow(deprecated)]
impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

/// Create a default axum router with Cors and tracing middleware
#[deprecated = "Default router will now return a `axum::Router::new()`. Previously, it would incorrectly attach cors and tracing middleware, which do no do anything..."]
pub fn default_router() -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ])
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST]);

    let tracing = TraceLayer::new_for_http()
        .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
        .on_response(trace::DefaultOnResponse::new().level(Level::INFO));

    return Router::new().layer(tracing).layer(cors);
}

/// Initializes tracing subscriber with format and env filter layer
pub fn init_tracing_subcriber() -> Result<()> {
    let fmt_layer = fmt::layer().with_target(false);
    let filter_layer = EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("info"))?;

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();

    return Ok(());
}

/// Attach tracing and cors middleware to a router
///
/// * `router`: the router to attach middleware to
pub fn attach_tracing_cors_middleware(router: Router) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ])
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ]);

    let tracing = TraceLayer::new_for_http()
        .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
        .on_response(trace::DefaultOnResponse::new().level(Level::INFO));

    return router.layer(ServiceBuilder::new().layer(tracing).layer(cors));
    // return router.layer(tracing).layer(cors);
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct HomeResponse {
    pub version: String,
}

#[utoipa::path(
    get,
    path = "/",
    responses(
        (status = 200, description = "Version of the server", body = HomeResponse),
        (status = 500, description = "Failed to get the vesion of the server", body = String),
    )
)]
pub async fn app_version() -> Result<Json<HomeResponse>, app_error_v2::AppError> {
    /// Simplified `Cargo.toml` structure
    #[derive(Deserialize)]
    struct CargoToml {
        pub package: PackageKeys,
    }

    #[derive(Deserialize)]
    struct PackageKeys {
        // pub name: String,
        pub version: String,
    }

    let mut file = File::open("Cargo.toml")
        .await
        .context("Failed to open Cargo.toml")?;
    let mut file_contents: String = Default::default();
    file.read_to_string(&mut file_contents)
        .await
        .context("Failed to read Cargo.toml")?;
    let cargo_toml = toml::from_str::<CargoToml>(file_contents.as_str())
        .context("Failed to parse Cargo.toml")?;

    return Ok(Json(HomeResponse {
        version: cargo_toml.package.version,
    }));
}

/// Start axum server on a specific port
///
/// * `listener`: listener
/// * `app`: app router
pub async fn axum_serve(listener: TcpListener, app: Router) -> Result<()> {
    Ok(axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?)
}

/// Axum graceful shutdown signal
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            println!("control c handler")
        },
        _ = terminate => {
            println!("terminate handler")
        },
    }
}

/// Generates an Open API spec
///
/// * `file_path`: the file location to save the API spec
pub fn generate_open_api_spec<T: OpenApi>(file_path: &str) -> Result<()> {
    let api_doc = T::openapi()
        .to_pretty_json()
        .context("Failed to generate open API spec")?;
    let mut file =
        std::fs::File::create(file_path).context("Failed to create open API spec file")?;
    file.write_all(api_doc.as_bytes())
        .context("Failed to write open api spec to file")?;
    return Ok(());
}

/// Generate open API spec from an open API object
pub fn generate_open_api_spec_from_open_api(
    open_api: utoipa::openapi::OpenApi,
    file_path: &str,
) -> Result<()> {
    let api_doc = open_api
        .to_pretty_json()
        .context("Failed to generate open API spec")?;
    let mut file =
        std::fs::File::create(file_path).context("Failed to create open API spec file")?;
    file.write_all(api_doc.as_bytes())
        .context("Failed to write open api spec to file")?;
    return Ok(());
}
