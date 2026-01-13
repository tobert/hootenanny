//! TLS certificate management for holler gateway.
//!
//! Provides self-signed certificate generation and RustlsConfig loading.

use anyhow::{Context, Result};
use hooteconf::infra::TlsConfig;
use std::path::PathBuf;

/// Resolved TLS certificate paths.
pub struct TlsCertPaths {
    pub cert: PathBuf,
    pub key: PathBuf,
}

impl TlsCertPaths {
    /// Resolve certificate paths from config, using XDG defaults if not specified.
    pub fn from_config(config: &TlsConfig) -> Result<Self> {
        let cert = config
            .resolved_cert_path()
            .context("Could not determine certificate path (HOME not set?)")?;
        let key = config
            .resolved_key_path()
            .context("Could not determine key path (HOME not set?)")?;

        Ok(Self { cert, key })
    }

    /// Check if both cert and key exist.
    pub fn exists(&self) -> bool {
        self.cert.exists() && self.key.exists()
    }
}

/// Generate self-signed certificate and key.
pub fn generate_self_signed(hostname: &str, paths: &TlsCertPaths) -> Result<()> {
    use rcgen::{generate_simple_self_signed, CertifiedKey};

    // Create parent directories
    if let Some(parent) = paths.cert.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create cert directory: {}", parent.display()))?;
    }

    let subject_alt_names = vec![
        hostname.to_string(),
        "localhost".to_string(),
        "127.0.0.1".to_string(),
    ];

    let CertifiedKey { cert, key_pair } = generate_simple_self_signed(subject_alt_names)
        .context("Failed to generate self-signed certificate")?;

    std::fs::write(&paths.cert, cert.pem())
        .with_context(|| format!("Failed to write certificate to {}", paths.cert.display()))?;

    std::fs::write(&paths.key, key_pair.serialize_pem())
        .with_context(|| format!("Failed to write private key to {}", paths.key.display()))?;

    Ok(())
}

/// Load TLS configuration from certificate files.
pub async fn load_rustls_config(
    config: &TlsConfig,
) -> Result<axum_server::tls_rustls::RustlsConfig> {
    let paths = TlsCertPaths::from_config(config)?;

    if !paths.exists() {
        anyhow::bail!(
            "TLS enabled but certificates not found.\n\
             Expected:\n  cert: {}\n  key: {}\n\n\
             Generate certificates with:\n  holler generate-cert --hostname <your-hostname>",
            paths.cert.display(),
            paths.key.display()
        );
    }

    axum_server::tls_rustls::RustlsConfig::from_pem_file(&paths.cert, &paths.key)
        .await
        .with_context(|| {
            format!(
                "Failed to load TLS config from {} and {}",
                paths.cert.display(),
                paths.key.display()
            )
        })
}
