use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// GPU utilization and memory statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuStats {
    /// GPU utilization percentage (0-100)
    pub utilization_percent: u8,
    /// VRAM used in bytes
    pub vram_used_bytes: u64,
    /// VRAM total in bytes
    pub vram_total_bytes: u64,
    /// When these stats were collected
    #[serde(skip, default = "Instant::now")]
    pub last_updated: Instant,
}

impl GpuStats {
    /// VRAM used in gigabytes (for display)
    pub fn vram_used_gb(&self) -> f64 {
        self.vram_used_bytes as f64 / 1_073_741_824.0
    }

    /// VRAM total in gigabytes (for display)
    pub fn vram_total_gb(&self) -> f64 {
        self.vram_total_bytes as f64 / 1_073_741_824.0
    }

    /// VRAM usage percentage
    pub fn vram_percent(&self) -> f64 {
        (self.vram_used_bytes as f64 / self.vram_total_bytes as f64) * 100.0
    }
}

/// Response from rocm-smi --json
#[derive(Debug, Deserialize)]
struct RocmSmiResponse {
    #[serde(flatten)]
    cards: std::collections::HashMap<String, CardStats>,
}

#[derive(Debug, Deserialize)]
struct CardStats {
    #[serde(rename = "GPU use (%)")]
    gpu_use: String,
    #[serde(rename = "VRAM Total Memory (B)")]
    vram_total: String,
    #[serde(rename = "VRAM Total Used Memory (B)")]
    vram_used: String,
}

/// GPU stats with historical context
#[derive(Debug, Clone, Serialize)]
pub struct GpuStatsWithHistory {
    /// Current snapshot
    pub current: Option<GpuStats>,
    /// Last 5 samples (10 seconds at 2s poll interval)
    pub utilization_10s: Vec<u8>,
    /// 1-minute mean statistics
    pub mean_1m: Option<GpuMeanStats>,
}

/// Mean statistics over a time window
#[derive(Debug, Clone, Serialize)]
pub struct GpuMeanStats {
    pub utilization: f64,
    pub vram_percent: f64,
}

/// Background GPU monitor with periodic polling
pub struct GpuMonitor {
    stats: Arc<RwLock<Option<GpuStats>>>,
    history: Arc<RwLock<VecDeque<GpuStats>>>,
    shutdown: CancellationToken,
}

impl GpuMonitor {
    /// Start the background GPU monitor
    pub fn start() -> Self {
        let stats = Arc::new(RwLock::new(None));
        let history = Arc::new(RwLock::new(VecDeque::with_capacity(30)));
        let shutdown = CancellationToken::new();

        tokio::spawn(poll_loop(stats.clone(), history.clone(), shutdown.clone()));

        Self { stats, history, shutdown }
    }

    /// Get the most recent GPU stats
    pub async fn current_stats(&self) -> Option<GpuStats> {
        self.stats.read().await.clone()
    }

    /// Get GPU stats with historical context
    pub async fn stats_with_history(&self) -> GpuStatsWithHistory {
        let current = self.stats.read().await.clone();
        let history = self.history.read().await;

        // Last 5 samples (10 seconds at 2s poll)
        let utilization_10s: Vec<u8> = history
            .iter()
            .rev()
            .take(5)
            .map(|s| s.utilization_percent)
            .collect();

        // Calculate 1-minute mean (up to 30 samples at 2s poll)
        let mean_1m = if !history.is_empty() {
            let sum_utilization: u64 = history.iter().map(|s| s.utilization_percent as u64).sum();
            let sum_vram_percent: f64 = history.iter().map(|s| s.vram_percent()).sum();
            let count = history.len() as f64;

            Some(GpuMeanStats {
                utilization: sum_utilization as f64 / count,
                vram_percent: sum_vram_percent / count,
            })
        } else {
            None
        };

        GpuStatsWithHistory {
            current,
            utilization_10s,
            mean_1m,
        }
    }

    /// Shutdown the background poller
    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }
}

impl Drop for GpuMonitor {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

async fn poll_loop(
    stats: Arc<RwLock<Option<GpuStats>>>,
    history: Arc<RwLock<VecDeque<GpuStats>>>,
    shutdown: CancellationToken,
) {
    const POLL_INTERVAL: Duration = Duration::from_secs(2);
    const MAX_HISTORY_SAMPLES: usize = 30; // 1 minute at 2s poll interval

    debug!("GPU monitor started");

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                debug!("GPU monitor shutting down");
                break;
            }
            _ = sleep(POLL_INTERVAL) => {
                match fetch_gpu_stats().await {
                    Ok(new_stats) => {
                        // Update current stats
                        *stats.write().await = Some(new_stats.clone());

                        // Add to history ring buffer
                        let mut hist = history.write().await;
                        hist.push_back(new_stats);

                        // Keep only last 30 samples (1 minute)
                        if hist.len() > MAX_HISTORY_SAMPLES {
                            hist.pop_front();
                        }
                    }
                    Err(e) => {
                        warn!("Failed to fetch GPU stats: {:#}", e);
                    }
                }
            }
        }
    }
}

async fn fetch_gpu_stats() -> Result<GpuStats> {
    let output = tokio::process::Command::new("/opt/rocm/bin/rocm-smi")
        .args(["--showuse", "--showmeminfo", "vram", "--json"])
        .output()
        .await
        .context("Failed to execute rocm-smi")?;

    if !output.status.success() {
        anyhow::bail!("rocm-smi exited with status {}", output.status);
    }

    let stdout = String::from_utf8(output.stdout).context("Invalid UTF-8 from rocm-smi")?;

    // rocm-smi prints warnings to stdout before JSON, so we need to find the JSON part
    let json_start = stdout
        .find('{')
        .context("No JSON object found in rocm-smi output")?;
    let json_str = &stdout[json_start..];

    let response: RocmSmiResponse =
        serde_json::from_str(json_str).context("Failed to parse rocm-smi JSON")?;

    // Get the first card (card0)
    let card = response
        .cards
        .get("card0")
        .context("No card0 in rocm-smi output")?;

    let utilization_percent = card
        .gpu_use
        .parse::<u8>()
        .context("Failed to parse GPU utilization")?;

    let vram_total_bytes = card
        .vram_total
        .parse::<u64>()
        .context("Failed to parse VRAM total")?;

    let vram_used_bytes = card
        .vram_used
        .parse::<u64>()
        .context("Failed to parse VRAM used")?;

    Ok(GpuStats {
        utilization_percent,
        vram_used_bytes,
        vram_total_bytes,
        last_updated: Instant::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rocm_smi_response() {
        let json = r#"{"card0": {"GPU use (%)": "100", "VRAM Total Memory (B)": "103079215104", "VRAM Total Used Memory (B)": "69830000640"}}"#;

        let response: RocmSmiResponse = serde_json::from_str(json).unwrap();
        let card = response.cards.get("card0").unwrap();

        assert_eq!(card.gpu_use, "100");
        assert_eq!(card.vram_total, "103079215104");
        assert_eq!(card.vram_used, "69830000640");
    }

    #[test]
    fn test_gpu_stats_conversions() {
        let stats = GpuStats {
            utilization_percent: 75,
            vram_used_bytes: 50_000_000_000,
            vram_total_bytes: 100_000_000_000,
            last_updated: Instant::now(),
        };

        assert!((stats.vram_used_gb() - 46.57).abs() < 0.1);
        assert!((stats.vram_total_gb() - 93.13).abs() < 0.1);
        assert_eq!(stats.vram_percent(), 50.0);
    }
}
