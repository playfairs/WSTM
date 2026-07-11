use anyhow::Result;
use tracing_subscriber::{fmt, EnvFilter};

fn main() -> Result<()> {
    fmt().with_env_filter(EnvFilter::from_default_env()).init();
    let app = wstm::app::WstmApp::new()?;
    app.run()?;
    Ok(())
}
