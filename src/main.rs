mod config;
mod gemini;
mod resource;
mod spartan;

use anyhow::{Result, anyhow};

use config::Config;
use resource::ResourceService;

#[tokio::main]
async fn main() -> Result<()> {
    install_crypto_provider()?;
    let config = Config::from_environment()?;
    let resources = ResourceService::new(config.root, config.media_origin)?;
    let gemini = gemini::serve(
        &config.gemini_address,
        config.hostname.clone(),
        &config.certificate_directory,
        resources.clone(),
    );
    let spartan = spartan::serve(&config.spartan_address, config.hostname, resources);

    tokio::select! {
        result = gemini => result,
        result = spartan => result,
        _ = tokio::signal::ctrl_c() => Ok(()),
    }
}

fn install_crypto_provider() -> Result<()> {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return Ok(());
    }

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow!("could not install the Rustls AWS-LC crypto provider"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_rustls_crypto_provider() {
        install_crypto_provider().unwrap();
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());

        // Building TLS configuration panicked in production when both providers
        // were enabled and no process-wide default had been selected.
        let _ = rustls::ServerConfig::builder();
    }
}
