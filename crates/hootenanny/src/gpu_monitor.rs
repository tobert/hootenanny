use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// GPU and system status from the observer service (localhost:2099)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverStatus {
    pub timestamp: String,
    pub summary: String,
    pub health: String,
    pub gpu: GpuStatus,
    pub system: SystemStatus,
    pub services: Vec<ServiceInfo>,
    pub sparklines: Sparklines,
    pub trends: Trends,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuStatus {
    pub status: String,
    pub vram_used_gb: f64,
    pub vram_total_gb: f64,
    pub util_pct: u8,
    pub temp_c: u8,
    pub power_w: u16,
    pub bandwidth_gbs: f64,
    pub bottleneck: String,
    pub oom_risk: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub mem_available_gb: f64,
    pub mem_total_gb: f64,
    pub mem_pressure: String,
    pub swap_used_gb: f64,
    pub load_1m: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub port: u16,
    pub vram_gb: f64,
    pub model: String,
    #[serde(rename = "type")]
    pub service_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sparklines {
    pub util: SparklineData,
    pub temp: SparklineData,
    pub power: SparklineData,
    pub vram: SparklineData,
    pub window_seconds: u32,
    pub sample_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SparklineData {
    pub min: f64,
    pub max: f64,
    pub current: f64,
    pub peak: f64,
    pub avg: f64,
    pub stddev: f64,
    #[serde(default)]
    pub delta: Option<f64>,
    pub spark: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trends {
    pub temp: String,
    pub power: String,
    pub activity: String,
}

/// Client for the GPU observer service
pub struct GpuMonitor {
    client: Client,
    base_url: String,
}

impl GpuMonitor {
    /// Create a new GPU monitor client
    pub fn new() -> Self {
        Self::with_url("http://127.0.0.1:2099")
    }

    /// Create with custom URL
    pub fn with_url(url: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: url.to_string(),
        }
    }

    /// Fetch current status from the observer service
    pub async fn fetch_status(&self) -> Result<ObserverStatus> {
        let response = self.client
            .get(&self.base_url)
            .send()
            .await
            .context("Failed to connect to GPU observer service")?;

        if !response.status().is_success() {
            anyhow::bail!("Observer service returned status {}", response.status());
        }

        response
            .json()
            .await
            .context("Failed to parse observer response")
    }

    /// Get a simple health check
    pub async fn health(&self) -> Result<String> {
        let url = format!("{}/health", self.base_url);
        let response = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to GPU observer service")?;

        response
            .text()
            .await
            .context("Failed to read health response")
    }

    /// Check if the observer service is available
    pub async fn is_available(&self) -> bool {
        self.health().await.is_ok()
    }
}

impl Default for GpuMonitor {
    fn default() -> Self {
        Self::new()
    }
}

// Legacy compatibility types for existing code

/// Legacy GPU stats (for backward compatibility with job_poll responses)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuStats {
    pub utilization_percent: u8,
    pub vram_used_bytes: u64,
    pub vram_total_bytes: u64,
}

impl GpuStats {
    pub fn vram_used_gb(&self) -> f64 {
        self.vram_used_bytes as f64 / 1_073_741_824.0
    }

    pub fn vram_total_gb(&self) -> f64 {
        self.vram_total_bytes as f64 / 1_073_741_824.0
    }

    pub fn vram_percent(&self) -> f64 {
        if self.vram_total_bytes == 0 {
            0.0
        } else {
            (self.vram_used_bytes as f64 / self.vram_total_bytes as f64) * 100.0
        }
    }
}

impl From<&GpuStatus> for GpuStats {
    fn from(status: &GpuStatus) -> Self {
        Self {
            utilization_percent: status.util_pct,
            vram_used_bytes: (status.vram_used_gb * 1_073_741_824.0) as u64,
            vram_total_bytes: (status.vram_total_gb * 1_073_741_824.0) as u64,
        }
    }
}

/// Legacy stats with history (for backward compatibility)
#[derive(Debug, Clone, Serialize)]
pub struct GpuStatsWithHistory {
    pub current: Option<GpuStats>,
    pub utilization_10s: Vec<u8>,
    pub mean_1m: Option<GpuMeanStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuMeanStats {
    pub utilization: f64,
    pub vram_percent: f64,
}

impl GpuMonitor {
    /// Legacy method: get stats with history (fetches from observer)
    pub async fn stats_with_history(&self) -> GpuStatsWithHistory {
        match self.fetch_status().await {
            Ok(status) => {
                let current = Some(GpuStats::from(&status.gpu));

                // The observer provides sparklines which encode recent history
                // We don't have individual samples, but we have the stats
                let mean_1m = Some(GpuMeanStats {
                    utilization: status.sparklines.util.avg,
                    vram_percent: (status.gpu.vram_used_gb / status.gpu.vram_total_gb) * 100.0,
                });

                GpuStatsWithHistory {
                    current,
                    utilization_10s: vec![status.gpu.util_pct], // Just current, sparkline has visual
                    mean_1m,
                }
            }
            Err(e) => {
                tracing::warn!("Failed to fetch GPU stats: {:#}", e);
                GpuStatsWithHistory {
                    current: None,
                    utilization_10s: vec![],
                    mean_1m: None,
                }
            }
        }
    }

    /// Legacy method: get current stats
    pub async fn current_stats(&self) -> Option<GpuStats> {
        self.fetch_status()
            .await
            .ok()
            .map(|s| GpuStats::from(&s.gpu))
    }

    /// No-op for compatibility (observer handles its own lifecycle)
    pub fn shutdown(&self) {
        // Observer service manages itself
    }

    /// Compatibility: "start" just returns self (no background task needed)
    pub fn start() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_stats_conversions() {
        let stats = GpuStats {
            utilization_percent: 75,
            vram_used_bytes: 50_000_000_000,
            vram_total_bytes: 100_000_000_000,
        };

        assert!((stats.vram_used_gb() - 46.57).abs() < 0.1);
        assert!((stats.vram_total_gb() - 93.13).abs() < 0.1);
        assert_eq!(stats.vram_percent(), 50.0);
    }

    #[test]
    fn test_gpu_status_to_stats() {
        let status = GpuStatus {
            status: "idle".to_string(),
            vram_used_gb: 49.5,
            vram_total_gb: 96.0,
            util_pct: 5,
            temp_c: 30,
            power_w: 50,
            bandwidth_gbs: 100.0,
            bottleneck: "none".to_string(),
            oom_risk: "none".to_string(),
        };

        let stats = GpuStats::from(&status);
        assert_eq!(stats.utilization_percent, 5);
        assert!((stats.vram_used_gb() - 49.5).abs() < 0.1);
        assert!((stats.vram_total_gb() - 96.0).abs() < 0.1);
    }
}
