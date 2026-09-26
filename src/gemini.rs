use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::FutureExt;
use rcgen::generate_simple_self_signed;
use tokio::fs;
use twinstar::{Meta, Request, Response, ResponseHeader, Server, Status};
use url::Url;

use crate::resource::{Resource, ResourceService, ResourceStatus};

pub async fn ensure_certificate(directory: &Path, hostname: &str) -> Result<()> {
    let certificate_path = directory.join("cert.pem");
    let key_path = directory.join("key.pem");
    let certificate_exists = fs::try_exists(&certificate_path).await?;
    let key_exists = fs::try_exists(&key_path).await?;

    if certificate_exists && key_exists {
        return Ok(());
    }
    if certificate_exists || key_exists {
        anyhow::bail!("Gemini certificate and key must either both exist or both be absent");
    }

    fs::create_dir_all(directory).await?;
    let certified_key = generate_simple_self_signed(vec![hostname.to_owned()])?;
    fs::write(&certificate_path, certified_key.cert.pem()).await?;
    fs::write(&key_path, certified_key.signing_key.serialize_pem()).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600)).await?;
    }

    Ok(())
}

pub async fn serve(
    address: &str,
    hostname: String,
    certificate_directory: &Path,
    resources: ResourceService,
) -> Result<()> {
    ensure_certificate(certificate_directory, &hostname).await?;
    let resources = Arc::new(resources);
    let handler_hostname = hostname.clone();

    Server::bind(address)
        .set_tls_dir(certificate_directory)
        .set_timeout(Duration::from_secs(10))
        .override_complex_body_timeout(Some(Duration::from_secs(30)))
        .add_route("/", move |request| {
            let resources = Arc::clone(&resources);
            let hostname = handler_hostname.clone();
            async move { handle(request, &hostname, &resources).await }.boxed()
        })
        .serve()
        .await
}

async fn handle(request: Request, hostname: &str, resources: &ResourceService) -> Result<Response> {
    let request_url = Url::parse(&request.uri().to_string()).context("Invalid Gemini URL")?;
    let requested_host_matches = request_url
        .host_str()
        .is_some_and(|requested_host| requested_host.eq_ignore_ascii_case(hostname));
    let server_name_matches = request
        .server_name()
        .is_some_and(|server_name| server_name.eq_ignore_ascii_case(hostname));
    if !requested_host_matches || !server_name_matches {
        return Ok(Response::bad_request_lossy("Host not served"));
    }

    let resource = resources.get(request_url.path(), request_url.query()).await;
    Ok(response(resource))
}

fn response(resource: Resource) -> Response {
    match resource.status {
        ResourceStatus::Success => match resource.media_type.parse::<twinstar::mime::Mime>() {
            Ok(media_type) => Response::success(&media_type, resource.body),
            Err(_) => Response::cgi_error_lossy("Invalid media type"),
        },
        ResourceStatus::Redirect => Response::new(ResponseHeader {
            status: Status::REDIRECT_PERMANENT,
            meta: Meta::new_lossy(resource.meta),
        }),
        ResourceStatus::NotFound => Response::not_found(),
        ResourceStatus::BadRequest => Response::bad_request_lossy(resource.meta),
        ResourceStatus::TemporaryFailure => Response::cgi_error_lossy(resource.meta),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creates_and_reuses_certificate() {
        let directory = tempfile::tempdir().unwrap();
        ensure_certificate(directory.path(), "example.com")
            .await
            .unwrap();
        let certificate = fs::read(directory.path().join("cert.pem")).await.unwrap();

        ensure_certificate(directory.path(), "example.com")
            .await
            .unwrap();

        assert_eq!(
            fs::read(directory.path().join("cert.pem")).await.unwrap(),
            certificate
        );
        assert!(
            fs::try_exists(directory.path().join("key.pem"))
                .await
                .unwrap()
        );
    }
}
