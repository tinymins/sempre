use std::{fs, path::PathBuf, sync::Arc};

use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::stream;
use tokio::{fs::File, io::AsyncReadExt as _};

use crate::api::{AppState, api_error};

const CHUNK_SIZE: usize = 64 * 1024;

pub(crate) fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/bundle/export", get(export))
}

async fn export(State(state): State<Arc<AppState>>) -> Response {
    let manager = Arc::clone(&state.manager);
    let result = match tokio::task::spawn_blocking(move || manager.export_bundle()).await {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "BUNDLE_EXPORT_FAILED",
                error.to_string(),
            );
        }
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "BUNDLE_EXPORT_FAILED",
                error.to_string(),
            );
        }
    };
    let file = match File::open(&result.archive).await {
        Ok(file) => file,
        Err(error) => {
            let _ = fs::remove_file(&result.archive);
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "BUNDLE_EXPORT_FAILED",
                error.to_string(),
            );
        }
    };
    let disposition = match HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        result.download_name
    )) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_file(&result.archive);
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "BUNDLE_EXPORT_FAILED",
                error.to_string(),
            );
        }
    };
    let body = Body::from_stream(stream::try_unfold(
        ArchiveReader {
            file,
            path: result.archive,
        },
        |mut reader| async move {
            let mut chunk = vec![0; CHUNK_SIZE];
            let size = reader.file.read(&mut chunk).await?;
            if size == 0 {
                return Ok(None);
            }
            chunk.truncate(size);
            Ok::<_, std::io::Error>(Some((Bytes::from(chunk), reader)))
        },
    ));
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/zip"),
            ),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        body,
    )
        .into_response()
}

struct ArchiveReader {
    file: File,
    path: PathBuf,
}

impl Drop for ArchiveReader {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
