use std::{env, fs, path::PathBuf};

use axum::{Json, Router, http::HeaderMap, http::StatusCode, routing::get};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Config {
    experimental: Experimental,
}

#[derive(Deserialize)]
struct Experimental {
    clash_api: ClashApi,
}

#[derive(Clone, Deserialize)]
struct ClashApi {
    external_controller: String,
    secret: String,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("ERROR: {error}");
        std::process::exit(2);
    }
}

async fn run() -> Result<(), String> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    match arguments.first().map(String::as_str) {
        Some("version") => {
            println!("sing-box version 1.2.3");
            Ok(())
        }
        Some("check") => {
            validate_configuration(&arguments[1..])?;
            Ok(())
        }
        Some("run") => serve(configuration(&arguments[1..])?).await,
        Some(command) => Err(format!("unsupported command {command:?}")),
        None => Err("missing command".into()),
    }
}

fn configuration(arguments: &[String]) -> Result<Config, String> {
    let path = configuration_path(arguments)?;
    let data = fs::read(&path)
        .map_err(|error| format!("read configuration {}: {error}", path.display()))?;
    serde_json::from_slice(&data)
        .map_err(|error| format!("decode configuration {}: {error}", path.display()))
}

fn validate_configuration(arguments: &[String]) -> Result<(), String> {
    let path = configuration_path(arguments)?;
    let data = fs::read(&path)
        .map_err(|error| format!("read configuration {}: {error}", path.display()))?;
    serde_json::from_slice::<Value>(&data)
        .map(|_| ())
        .map_err(|error| format!("decode configuration {}: {error}", path.display()))
}

fn configuration_path(arguments: &[String]) -> Result<PathBuf, String> {
    let path = arguments
        .windows(2)
        .find(|pair| pair[0] == "-c")
        .map(|pair| PathBuf::from(&pair[1]))
        .ok_or_else(|| "configuration argument is missing".to_owned())?;
    if !path.is_file() {
        return Err(format!("configuration is unavailable: {}", path.display()));
    }
    Ok(path)
}

async fn serve(config: Config) -> Result<(), String> {
    let listener =
        tokio::net::TcpListener::bind(&config.experimental.clash_api.external_controller)
            .await
            .map_err(|error| format!("listen on control API: {error}"))?;
    let secret = config.experimental.clash_api.secret;
    let config_secret = secret.clone();
    let connection_secret = secret.clone();
    let app = Router::new()
        .route(
            "/version",
            get(move |headers: HeaderMap| {
                let secret = secret.clone();
                async move { version(&headers, &secret) }
            }),
        )
        .route(
            "/configs",
            get(move |headers: HeaderMap| {
                let secret = config_secret.clone();
                async move { empty_object(&headers, &secret) }
            }),
        )
        .route(
            "/connections",
            get(move |headers: HeaderMap| {
                let secret = connection_secret.clone();
                async move { empty_connections(&headers, &secret) }
            }),
        );
    println!("test core started");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await
        .map_err(|error| format!("serve control API: {error}"))
}

fn version(headers: &HeaderMap, secret: &str) -> Result<Json<Value>, StatusCode> {
    authorize(headers, secret)?;
    Ok(Json(json!({ "version": "1.2.3", "meta": false })))
}

fn empty_object(headers: &HeaderMap, secret: &str) -> Result<Json<Value>, StatusCode> {
    authorize(headers, secret)?;
    Ok(Json(json!({})))
}

fn empty_connections(headers: &HeaderMap, secret: &str) -> Result<Json<Value>, StatusCode> {
    authorize(headers, secret)?;
    Ok(Json(
        json!({ "connections": [], "downloadTotal": 0, "uploadTotal": 0 }),
    ))
}

fn authorize(headers: &HeaderMap, secret: &str) -> Result<(), StatusCode> {
    let expected = format!("Bearer {secret}");
    if headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        == Some(expected.as_str())
    {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

async fn shutdown() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { () = interrupt => {}, () = terminate => {} }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_requires_a_real_json_file() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("config.json");
        fs::write(
            &path,
            r#"{"experimental":{"clash_api":{"external_controller":"127.0.0.1:9090","secret":"test"}}}"#,
        )
        .expect("configuration");
        assert!(configuration(&["-c".into(), path.display().to_string()]).is_ok());
        assert!(validate_configuration(&["-c".into(), path.display().to_string()]).is_ok());
        assert!(configuration(&[]).is_err());
    }
}
