use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use percent_encoding::percent_decode_str;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use crate::resource::{Resource, ResourceService, ResourceStatus};

const MAXIMUM_REQUEST_LINE: u64 = 4096;

pub async fn serve(address: &str, hostname: String, resources: ResourceService) -> Result<()> {
    let listener = TcpListener::bind(address).await?;
    let resources = Arc::new(resources);
    let hostname = Arc::new(hostname);

    loop {
        let (stream, _) = listener.accept().await?;
        let resources = Arc::clone(&resources);
        let hostname = Arc::clone(&hostname);
        tokio::spawn(async move {
            let _ = tokio::time::timeout(
                Duration::from_secs(30),
                handle_connection(stream, &hostname, &resources),
            )
            .await;
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    hostname: &str,
    resources: &ResourceService,
) -> Result<()> {
    let mut reader = BufReader::new(stream);
    let mut line = Vec::new();
    let bytes_read = (&mut reader)
        .take(MAXIMUM_REQUEST_LINE + 2)
        .read_until(b'\n', &mut line)
        .await?;

    if bytes_read == 0 || bytes_read as u64 > MAXIMUM_REQUEST_LINE || !line.ends_with(b"\r\n") {
        write_error(reader.get_mut(), "4", "Invalid request").await?;
        return Ok(());
    }

    line.truncate(line.len() - 2);
    let Ok(line) = std::str::from_utf8(&line) else {
        write_error(reader.get_mut(), "4", "Invalid request").await?;
        return Ok(());
    };
    let parts: Vec<&str> = line.split(' ').collect();
    if parts.len() != 3 || !parts[0].eq_ignore_ascii_case(hostname) {
        write_error(reader.get_mut(), "4", "Host not served").await?;
        return Ok(());
    }

    let Ok(content_length) = parts[2].parse::<u64>() else {
        write_error(reader.get_mut(), "4", "Invalid content length").await?;
        return Ok(());
    };
    if content_length != 0 {
        write_error(reader.get_mut(), "4", "Uploads are not supported").await?;
        return Ok(());
    }
    if !parts[1].starts_with('/') {
        write_error(reader.get_mut(), "4", "Invalid path").await?;
        return Ok(());
    }

    let Ok(request_path) = percent_decode_str(parts[1]).decode_utf8() else {
        write_error(reader.get_mut(), "4", "Invalid path").await?;
        return Ok(());
    };
    let resource = resources.get(&request_path, None).await;
    write_response(reader.get_mut(), resource).await?;
    Ok(())
}

async fn write_response(stream: &mut TcpStream, resource: Resource) -> Result<()> {
    match resource.status {
        ResourceStatus::Success => {
            stream
                .write_all(format!("2 {}\r\n", resource.media_type).as_bytes())
                .await?;
            stream.write_all(&resource.body).await?;
        }
        ResourceStatus::Redirect => {
            write_error(stream, "3", &resource.meta).await?;
        }
        ResourceStatus::NotFound | ResourceStatus::BadRequest => {
            write_error(stream, "4", &resource.meta).await?;
        }
        ResourceStatus::TemporaryFailure => {
            write_error(stream, "5", &resource.meta).await?;
        }
    }
    Ok(())
}

async fn write_error(stream: &mut TcpStream, status: &str, meta: &str) -> Result<()> {
    let meta = meta.replace(['\r', '\n'], " ");
    stream
        .write_all(format!("{status} {meta}\r\n").as_bytes())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tokio::fs;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn exchange(request: &[u8], resources: ResourceService) -> Vec<u8> {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_connection(stream, "example.com", &resources)
                .await
                .unwrap();
        });

        let mut client = TcpStream::connect(address).await.unwrap();
        client.write_all(request).await.unwrap();
        client.shutdown().await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        server.await.unwrap();
        response
    }

    #[tokio::test]
    async fn serves_shared_static_resources() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("index.gmi"), "# Home\n")
            .await
            .unwrap();
        let resources = ResourceService::new(directory.path().into(), None).unwrap();

        let response = exchange(b"example.com / 0\r\n", resources).await;
        assert_eq!(response, b"2 text/gemini; charset=utf-8\r\n# Home\n");
    }

    #[tokio::test]
    async fn rejects_uploads() {
        let resources = ResourceService::new(PathBuf::new(), None).unwrap();
        let response = exchange(b"example.com /upload 5\r\nhello", resources).await;
        assert_eq!(response, b"4 Uploads are not supported\r\n");
    }

    #[tokio::test]
    async fn rejects_other_hosts() {
        let resources = ResourceService::new(PathBuf::new(), None).unwrap();
        let response = exchange(b"other.example / 0\r\n", resources).await;
        assert_eq!(response, b"4 Host not served\r\n");
    }
}
