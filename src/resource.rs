use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use futures_util::StreamExt;
use reqwest::{Client, StatusCode, redirect};
use tokio::fs;
use url::Url;

const MAXIMUM_MEDIA_SIZE: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceStatus {
    Success,
    Redirect,
    NotFound,
    BadRequest,
    TemporaryFailure,
}

#[derive(Debug)]
pub struct Resource {
    pub body: Vec<u8>,
    pub media_type: String,
    pub meta: String,
    pub status: ResourceStatus,
}

impl Resource {
    fn error(status: ResourceStatus, meta: &str) -> Self {
        Self {
            body: Vec::new(),
            media_type: String::new(),
            meta: meta.to_owned(),
            status,
        }
    }
}

#[derive(Clone)]
pub struct ResourceService {
    client: Client,
    media_origin: Option<Url>,
    root: Arc<PathBuf>,
    redirects: Arc<HashMap<String, String>>,
}

impl ResourceService {
    pub fn new(root: PathBuf, media_origin: Option<Url>) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(redirect::Policy::none())
            .build()?;

        let redirects_path = root.join("redirects.json");
        let redirects = match std::fs::read(&redirects_path) {
            Ok(body) => serde_json::from_slice::<HashMap<String, String>>(&body)
                .with_context(|| format!("Invalid redirect map: {}", redirects_path.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Unable to read {}", redirects_path.display()));
            }
        };

        Ok(Self {
            client,
            media_origin,
            root: Arc::new(root),
            redirects: Arc::new(redirects),
        })
    }

    #[cfg(test)]
    fn with_client(root: PathBuf, media_origin: Option<Url>, client: Client) -> Self {
        Self {
            client,
            media_origin,
            root: Arc::new(root),
            redirects: Arc::new(HashMap::new()),
        }
    }

    pub async fn get(&self, request_path: &str, query: Option<&str>) -> Resource {
        let request_path = if request_path.is_empty() {
            "/"
        } else {
            request_path
        };

        if !request_path.starts_with('/') || request_path.contains('\0') {
            return Resource::error(ResourceStatus::BadRequest, "Invalid path");
        }
        if request_path.starts_with("/media/") {
            return self.get_media(request_path, query).await;
        }

        self.get_file(request_path).await
    }

    async fn get_file(&self, request_path: &str) -> Resource {
        let Some(relative_path) = safe_relative_path(request_path) else {
            return Resource::error(ResourceStatus::BadRequest, "Invalid path");
        };
        // Validate paths before allowing configured redirects to override file handling.
        if let Some(target) = self.redirects.get(request_path) {
            return Resource::error(ResourceStatus::Redirect, target);
        }
        let mut file_path = self.root.join(&relative_path);

        let metadata = match fs::metadata(&file_path).await {
            Ok(metadata) => metadata,
            Err(error) => return file_error(&error),
        };

        if metadata.is_dir() {
            if !request_path.ends_with('/') {
                return Resource::error(
                    ResourceStatus::Redirect,
                    &format!("{}/", request_path.trim_end_matches('/')),
                );
            }
            file_path.push("index.gmi");
        } else if request_path.ends_with('/') {
            return Resource::error(ResourceStatus::Redirect, request_path.trim_end_matches('/'));
        } else if request_path.ends_with("/index.gmi") {
            return Resource::error(
                ResourceStatus::Redirect,
                request_path.trim_end_matches("index.gmi"),
            );
        }

        let body = match fs::read(&file_path).await {
            Ok(body) => body,
            Err(error) => return file_error(&error),
        };
        let media_type = if file_path
            .extension()
            .is_some_and(|extension| extension == "gmi")
        {
            "text/gemini; charset=utf-8".to_owned()
        } else {
            mime_guess::from_path(&file_path)
                .first_or_octet_stream()
                .to_string()
        };

        Resource {
            body,
            media_type,
            meta: String::new(),
            status: ResourceStatus::Success,
        }
    }

    async fn get_media(&self, request_path: &str, query: Option<&str>) -> Resource {
        let Some(origin) = &self.media_origin else {
            return Resource::error(ResourceStatus::TemporaryFailure, "Media origin unavailable");
        };

        let mut target = origin.clone();
        target.set_path(request_path);
        target.set_query(query);

        let response = match self
            .client
            .get(target)
            .header("User-Agent", "hans.gerwitz.com-calmserve/1")
            .send()
            .await
        {
            Ok(response) => response,
            Err(_) => {
                return Resource::error(
                    ResourceStatus::TemporaryFailure,
                    "Media origin unavailable",
                );
            }
        };

        match response.status() {
            status if status.is_success() => {
                let media_type = response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<mime_guess::mime::Mime>().ok())
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| {
                        mime_guess::from_path(request_path)
                            .first_or_octet_stream()
                            .to_string()
                    });
                let mut body = Vec::new();
                let mut stream = response.bytes_stream();

                while let Some(chunk) = stream.next().await {
                    let Ok(chunk) = chunk else {
                        return Resource::error(
                            ResourceStatus::TemporaryFailure,
                            "Media origin unavailable",
                        );
                    };
                    if body.len() + chunk.len() > MAXIMUM_MEDIA_SIZE {
                        return Resource::error(
                            ResourceStatus::TemporaryFailure,
                            "Media resource is too large",
                        );
                    }
                    body.extend_from_slice(&chunk);
                }

                Resource {
                    body,
                    media_type,
                    meta: String::new(),
                    status: ResourceStatus::Success,
                }
            }
            StatusCode::NOT_FOUND => Resource::error(ResourceStatus::NotFound, "Not found"),
            status if status.is_server_error() => {
                Resource::error(ResourceStatus::TemporaryFailure, "Media origin unavailable")
            }
            _ => Resource::error(ResourceStatus::NotFound, "Not found"),
        }
    }
}

fn safe_relative_path(request_path: &str) -> Option<PathBuf> {
    let mut relative = PathBuf::new();
    for component in Path::new(request_path.trim_start_matches('/')).components() {
        match component {
            Component::Normal(component) => relative.push(component),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(relative)
}

fn file_error(error: &std::io::Error) -> Resource {
    match error.kind() {
        std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied => {
            Resource::error(ResourceStatus::NotFound, "Not found")
        }
        _ => Resource::error(ResourceStatus::TemporaryFailure, "Unable to read resource"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn serves_static_indexes_and_redirects() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("writing"))
            .await
            .unwrap();
        fs::write(directory.path().join("index.gmi"), "# Home\n")
            .await
            .unwrap();
        fs::write(directory.path().join("writing/index.gmi"), "# Writing\n")
            .await
            .unwrap();
        let service = ResourceService::new(directory.path().into(), None).unwrap();

        let root = service.get("/", None).await;
        assert_eq!(root.status, ResourceStatus::Success);
        assert_eq!(root.media_type, "text/gemini; charset=utf-8");
        assert_eq!(root.body, b"# Home\n");

        let redirect = service.get("/writing", None).await;
        assert_eq!(redirect.status, ResourceStatus::Redirect);
        assert_eq!(redirect.meta, "/writing/");

        let writing = service.get("/writing/", None).await;
        assert_eq!(writing.status, ResourceStatus::Success);
        assert_eq!(writing.body, b"# Writing\n");
    }

    #[tokio::test]
    async fn configured_redirects_override_static_files_and_directory_redirects() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("writing"))
            .await
            .unwrap();
        fs::write(directory.path().join("writing/index.gmi"), "# Writing\n")
            .await
            .unwrap();
        fs::write(directory.path().join("2005-01-02-old.gmi"), "# Old\n")
            .await
            .unwrap();
        fs::write(
            directory.path().join("redirects.json"),
            r#"{
                "/writing/": "/notes/",
                "/writing": "/notes/",
                "/2005-01-02-old.gmi": "/notes/old/",
                "/writing/2006-03-04-missing.gmi": "gemini://example.com/notes/missing/"
            }"#,
        )
        .await
        .unwrap();
        let service = ResourceService::new(directory.path().into(), None).unwrap();

        for (path, target) in [
            ("/writing/", "/notes/"),
            ("/writing", "/notes/"),
            ("/2005-01-02-old.gmi", "/notes/old/"),
            (
                "/writing/2006-03-04-missing.gmi",
                "gemini://example.com/notes/missing/",
            ),
        ] {
            let resource = service.get(path, Some("ignored=query")).await;
            assert_eq!(resource.status, ResourceStatus::Redirect, "{path}");
            assert_eq!(resource.meta, target);
            assert!(resource.body.is_empty());
        }
    }

    #[tokio::test]
    async fn redirect_map_leaves_unmatched_static_resources_unchanged() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("index.gmi"), "# Home\n")
            .await
            .unwrap();
        fs::write(directory.path().join("notes.gmi"), "# Notes\n")
            .await
            .unwrap();
        fs::write(directory.path().join("robots.txt"), "User-agent: *\n")
            .await
            .unwrap();
        fs::write(
            directory.path().join("redirects.json"),
            r#"{"/writing/": "/notes/"}"#,
        )
        .await
        .unwrap();
        let service = ResourceService::new(directory.path().into(), None).unwrap();

        for (path, body, media_type) in [
            ("/", "# Home\n", "text/gemini; charset=utf-8"),
            ("/notes.gmi", "# Notes\n", "text/gemini; charset=utf-8"),
            ("/robots.txt", "User-agent: *\n", "text/plain"),
        ] {
            let resource = service.get(path, None).await;
            assert_eq!(resource.status, ResourceStatus::Success, "{path}");
            assert_eq!(resource.body, body.as_bytes());
            assert_eq!(resource.media_type, media_type);
        }
        assert_eq!(
            service.get("/writing", None).await.status,
            ResourceStatus::NotFound
        );
        assert_eq!(
            service.get("/missing.gmi", None).await.status,
            ResourceStatus::NotFound
        );
    }

    #[tokio::test]
    async fn redirect_map_does_not_bypass_path_validation() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("redirects.json"),
            r#"{
                "/../secret": "/notes/",
                "/writing/../../secret": "/notes/",
                "relative": "/notes/",
                "/null\u0000": "/notes/"
            }"#,
        )
        .await
        .unwrap();
        let service = ResourceService::new(directory.path().into(), None).unwrap();

        for path in ["/../secret", "/writing/../../secret", "relative", "/null\0"] {
            assert_eq!(
                service.get(path, None).await.status,
                ResourceStatus::BadRequest,
                "{path}"
            );
        }
    }

    #[tokio::test]
    async fn rejects_invalid_redirect_maps() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let directory = tempfile::tempdir().unwrap();
        for body in ["not json", "[]", r#"{"/writing/": 42}"#] {
            fs::write(directory.path().join("redirects.json"), body)
                .await
                .unwrap();
            let error = ResourceService::new(directory.path().into(), None)
                .err()
                .expect("invalid redirect map must fail initialization");
            assert!(error.to_string().contains("Invalid redirect map"));
        }
    }

    #[tokio::test]
    async fn proxies_media_with_its_content_type() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 1024];
            let size = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..size]);
            assert!(request.starts_with("GET /media/photo.jpg?size=large HTTP/1.1\r\n"));
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: 5\r\n\r\nimage",
                )
                .await
                .unwrap();
        });

        let origin = Url::parse(&format!("http://{address}")).unwrap();
        let client = Client::builder().build().unwrap();
        let service = ResourceService::with_client(PathBuf::new(), Some(origin), client);
        let resource = service.get("/media/photo.jpg", Some("size=large")).await;

        assert_eq!(resource.status, ResourceStatus::Success);
        assert_eq!(resource.media_type, "image/jpeg");
        assert_eq!(resource.body, b"image");
    }

    #[test]
    fn rejects_parent_paths() {
        assert!(safe_relative_path("/../secret").is_none());
    }
}
