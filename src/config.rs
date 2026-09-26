use std::env;
use std::path::PathBuf;

use anyhow::{Result, bail};
use url::Url;

#[derive(Debug)]
pub struct Config {
    pub certificate_directory: PathBuf,
    pub gemini_address: String,
    pub hostname: String,
    pub media_origin: Option<Url>,
    pub root: PathBuf,
    pub spartan_address: String,
}

impl Config {
    pub fn from_environment() -> Result<Self> {
        let media_origin = match env::var("MEDIA_ORIGIN_HOST") {
            Ok(host) if !host.is_empty() => Some(parse_media_origin(&host)?),
            _ => None,
        };

        Ok(Self {
            certificate_directory: environment(
                "GEMINI_CERTIFICATE_DIRECTORY",
                "/var/lib/smallweb/certificates",
            )
            .into(),
            gemini_address: environment("GEMINI_LISTEN", "0.0.0.0:1965"),
            hostname: environment("SMALLWEB_HOSTNAME", "hans.gerwitz.com"),
            media_origin,
            root: environment("SMALLWEB_ROOT", "_site/editions/gemini").into(),
            spartan_address: environment("SPARTAN_LISTEN", "0.0.0.0:3000"),
        })
    }
}

fn parse_media_origin(host: &str) -> Result<Url> {
    if host.contains(['/', '?', '#', '@']) {
        bail!("MEDIA_ORIGIN_HOST must be a hostname, optionally with a port");
    }

    let origin = Url::parse(&format!("https://{host}"))?;
    if origin.host_str().is_none() || origin.path() != "/" {
        bail!("invalid MEDIA_ORIGIN_HOST");
    }

    Ok(origin)
}

fn environment(name: &str, fallback: &str) -> String {
    env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_media_origin_host() {
        let origin = parse_media_origin("bucket.example.com:8443").unwrap();
        assert_eq!(origin.as_str(), "https://bucket.example.com:8443/");
    }

    #[test]
    fn rejects_media_origin_url() {
        assert!(parse_media_origin("https://example.com").is_err());
    }
}
