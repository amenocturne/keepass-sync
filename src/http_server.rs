use crate::{FilesystemRemote, Manifest, RemoteError, Revision};
use serde::Serialize;
use std::io;
use std::path::PathBuf;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

#[derive(Debug, Clone)]
pub struct HttpServerConfig {
    pub bind: String,
    pub remote_root: PathBuf,
    pub token: String,
}

#[derive(Debug)]
pub enum HttpServerError {
    Io(io::Error),
    Server(String),
    Remote(RemoteError),
    Json(serde_json::Error),
    InvalidRevision(crate::revision::RevisionError),
    MissingToken,
    EmptyToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RequestTarget {
    path: String,
    query: Vec<(String, String)>,
}

pub fn serve(config: HttpServerConfig) -> Result<(), HttpServerError> {
    if config.token.trim().is_empty() {
        return Err(HttpServerError::EmptyToken);
    }

    let server =
        Server::http(&config.bind).map_err(|error| HttpServerError::Server(error.to_string()))?;
    let remote = FilesystemRemote::new(config.remote_root);

    for request in server.incoming_requests() {
        if let Err(error) = handle_request(request, &remote, &config.token) {
            eprintln!("request failed: {error}");
        }
    }

    Ok(())
}

fn handle_request(
    mut request: Request,
    remote: &FilesystemRemote,
    token: &str,
) -> Result<(), HttpServerError> {
    let method = request.method().clone();
    let target = RequestTarget::parse(request.url());

    if method == Method::Get && target.path == "/health" {
        request.respond(text_response(StatusCode(200), "ok\n"))?;
        return Ok(());
    }

    if !is_authorized(&request, token) {
        request.respond(text_response(StatusCode(401), "unauthorized\n"))?;
        return Ok(());
    }

    match (method, target.path.as_str()) {
        (Method::Get, "/manifest") => match remote.current_revision()? {
            Some(_) => {
                let manifest = Manifest::read(remote.root().join("canonical/manifest.json"))?;
                request.respond(json_response(StatusCode(200), &manifest)?)?;
            }
            None => request.respond(text_response(StatusCode(404), "manifest not found\n"))?,
        },
        (Method::Get, "/canonical") => match remote.canonical_bytes() {
            Ok(bytes) => request.respond(bytes_response(StatusCode(200), bytes))?,
            Err(RemoteError::NoCanonicalDatabase) => request.respond(text_response(
                StatusCode(404),
                "canonical database not found\n",
            ))?,
            Err(error) => return Err(error.into()),
        },
        (Method::Get, "/incoming") => {
            let mut incoming = Vec::new();
            for database in remote.incoming_databases()? {
                let revision = Revision::from_file(&database.path)?;
                let size = database.path.metadata().map_err(RemoteError::Io)?.len();
                incoming.push(IncomingListEntry {
                    device_id: database.device_id,
                    revision,
                    size,
                });
            }
            request.respond(json_response(
                StatusCode(200),
                &IncomingListResponse { incoming },
            )?)?;
        }
        (Method::Get, path) if path.starts_with("/incoming/") => {
            let Some((device_id, revision)) = parse_incoming_path(path) else {
                request.respond(text_response(StatusCode(404), "not found\n"))?;
                return Ok(());
            };
            let revision = Revision::parse(revision)?;
            match remote.incoming_file(&device_id, &revision) {
                Ok(bytes) => request.respond(bytes_response(StatusCode(200), bytes))?,
                Err(RemoteError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                    request.respond(text_response(StatusCode(404), "incoming not found\n"))?
                }
                Err(error) => return Err(error.into()),
            }
        }
        (Method::Put, "/canonical") => {
            let device_id = required_query(&target, "device_id")
                .unwrap_or_else(|| "unknown-device".to_string());
            let revision = required_query(&target, "revision")
                .map(Revision::parse)
                .transpose()?
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing revision"))?;
            let base_revision = required_query(&target, "base_revision")
                .filter(|value| !value.is_empty())
                .map(Revision::parse)
                .transpose()?;
            let mut bytes = Vec::new();
            request.as_reader().read_to_end(&mut bytes)?;

            match remote.publish_bytes_if_base_matches(
                &bytes,
                base_revision.as_ref(),
                &revision,
                &device_id,
            ) {
                Ok(manifest) => request.respond(json_response(StatusCode(200), &manifest)?)?,
                Err(RemoteError::BaseRevisionMismatch { expected, actual }) => request.respond(
                    json_response(StatusCode(409), &ConflictResponse { expected, actual })?,
                )?,
                Err(error) => return Err(error.into()),
            }
        }
        (Method::Put, path) if path.starts_with("/incoming/") => {
            let Some((device_id, revision)) = parse_incoming_path(path) else {
                request.respond(text_response(StatusCode(404), "not found\n"))?;
                return Ok(());
            };
            let revision = Revision::parse(revision)?;
            let mut bytes = Vec::new();
            request.as_reader().read_to_end(&mut bytes)?;
            let path = remote.preserve_incoming_bytes(&bytes, &revision, &device_id)?;
            request.respond(json_response(
                StatusCode(200),
                &IncomingResponse {
                    path: path.display().to_string(),
                },
            )?)?;
        }
        _ => request.respond(text_response(StatusCode(404), "not found\n"))?,
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct ConflictResponse {
    expected: Option<Revision>,
    actual: Option<Revision>,
}

#[derive(Debug, Serialize)]
struct IncomingResponse {
    path: String,
}

#[derive(Debug, Serialize)]
struct IncomingListResponse {
    incoming: Vec<IncomingListEntry>,
}

#[derive(Debug, Serialize)]
struct IncomingListEntry {
    device_id: String,
    revision: Revision,
    size: u64,
}

impl RequestTarget {
    fn parse(url: &str) -> Self {
        let (path, query) = url.split_once('?').unwrap_or((url, ""));
        let query = query
            .split('&')
            .filter(|part| !part.is_empty())
            .filter_map(|part| {
                let (key, value) = part.split_once('=').unwrap_or((part, ""));
                Some((url_decode(key)?, url_decode(value)?))
            })
            .collect();
        Self {
            path: path.to_string(),
            query,
        }
    }
}

fn required_query(target: &RequestTarget, key: &str) -> Option<String> {
    target
        .query
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.clone())
}

fn parse_incoming_path(path: &str) -> Option<(String, String)> {
    let parts = path
        .trim_start_matches('/')
        .split('/')
        .map(url_decode)
        .collect::<Option<Vec<_>>>()?;
    match parts.as_slice() {
        [incoming, device_id, revision] if incoming == "incoming" => {
            Some((device_id.clone(), revision.clone()))
        }
        _ => None,
    }
}

fn is_authorized(request: &Request, token: &str) -> bool {
    let expected = format!("Bearer {token}");
    request.headers().iter().any(|header| {
        header.field.equiv("Authorization") && header.value.as_str() == expected.as_str()
    })
}

fn json_response<T: Serialize>(
    status: StatusCode,
    value: &T,
) -> Result<Response<std::io::Cursor<Vec<u8>>>, HttpServerError> {
    let body = serde_json::to_vec_pretty(value)?;
    Ok(Response::from_data(body)
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json")))
}

fn bytes_response(status: StatusCode, bytes: Vec<u8>) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(bytes)
        .with_status_code(status)
        .with_header(header("Content-Type", "application/octet-stream"))
}

fn text_response(status: StatusCode, text: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(text.to_string())
        .with_status_code(status)
        .with_header(header("Content-Type", "text/plain; charset=utf-8"))
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static HTTP header is valid")
}

fn url_decode(value: &str) -> Option<String> {
    let mut decoded = Vec::with_capacity(value.len());
    let mut bytes = value.as_bytes().iter().copied();
    while let Some(byte) = bytes.next() {
        match byte {
            b'+' => decoded.push(b' '),
            b'%' => {
                let high = bytes.next()?;
                let low = bytes.next()?;
                decoded.push(hex_value(high)? * 16 + hex_value(low)?);
            }
            other => decoded.push(other),
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

impl From<io::Error> for HttpServerError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<RemoteError> for HttpServerError {
    fn from(error: RemoteError) -> Self {
        Self::Remote(error)
    }
}

impl From<crate::manifest::ManifestError> for HttpServerError {
    fn from(error: crate::manifest::ManifestError) -> Self {
        Self::Remote(RemoteError::Manifest(error))
    }
}

impl From<serde_json::Error> for HttpServerError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<crate::revision::RevisionError> for HttpServerError {
    fn from(error: crate::revision::RevisionError) -> Self {
        Self::InvalidRevision(error)
    }
}

impl std::fmt::Display for HttpServerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "HTTP server IO error: {error}"),
            Self::Server(error) => write!(formatter, "HTTP server error: {error}"),
            Self::Remote(error) => write!(formatter, "{error}"),
            Self::Json(error) => write!(formatter, "HTTP JSON error: {error}"),
            Self::InvalidRevision(error) => write!(formatter, "invalid revision: {error}"),
            Self::MissingToken => formatter
                .write_str("missing token; pass --token, --token-file, or KEEPASS_SYNC_TOKEN"),
            Self::EmptyToken => formatter.write_str("sync token cannot be empty"),
        }
    }
}

impl std::error::Error for HttpServerError {}

#[cfg(test)]
mod tests {
    use super::{RequestTarget, parse_incoming_path};

    #[test]
    fn parses_request_target_query() {
        let target =
            RequestTarget::parse("/canonical?device_id=Pixel+8&base_revision=sha256%3Aabc");

        assert_eq!(target.path, "/canonical");
        assert_eq!(
            target.query,
            vec![
                ("device_id".to_string(), "Pixel 8".to_string()),
                ("base_revision".to_string(), "sha256:abc".to_string())
            ]
        );
    }

    #[test]
    fn parses_incoming_path() {
        assert_eq!(
            parse_incoming_path("/incoming/pixel/sha256%3Aabc"),
            Some(("pixel".to_string(), "sha256:abc".to_string()))
        );
    }
}
