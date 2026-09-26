mod config;
mod gemini;
mod resource;
mod spartan;

use anyhow::Result;

use config::Config;
use resource::ResourceService;

#[tokio::main]
async fn main() -> Result<()> {
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
