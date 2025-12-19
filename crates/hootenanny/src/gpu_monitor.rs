#![allow(dead_code)]

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
    pub util_pct: f64,
    pub temp_c: f64,
    pub power_w: f64,
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
    /// Create a new GPU monitor client (connects to localhost:2099)
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
        let response = self
            .client
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
        let response = self
            .client
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_real_observer() {
        let monitor = GpuMonitor::new();
        match monitor.fetch_status().await {
            Ok(status) => {
                println!("✅ Got status: {}", status.summary);
                println!("   Health: {}", status.health);
                println!(
                    "   GPU: {}% util, {:.1}/{:.1}GB VRAM",
                    status.gpu.util_pct, status.gpu.vram_used_gb, status.gpu.vram_total_gb
                );
                assert!(!status.summary.is_empty());
            }
            Err(e) => {
                println!("❌ Failed to fetch: {:#}", e);
                // Don't fail the test - observer might not be running in CI
            }
        }
    }

    #[test]
    fn test_deserialize_observer_status() {
        let json = r#"{
            "timestamp": "2025-12-07T15:07:57.132840",
            "summary": "GPU idle 1.0% | 49.5/96GB | 28°C | 10 services 49GB | good",
            "health": "good",
            "gpu": {
                "status": "idle",
                "vram_used_gb": 49.5,
                "vram_total_gb": 96,
                "util_pct": 1,
                "temp_c": 28,
                "power_w": 33,
                "bandwidth_gbs": 57,
                "bottleneck": "none",
                "oom_risk": "none"
            },
            "system": {
                "mem_available_gb": 22.2,
                "mem_total_gb": 31.1,
                "mem_pressure": "low",
                "swap_used_gb": 2.6,
                "load_1m": 0.57
            },
            "services": [
                {
                    "name": "orpheus-base",
                    "port": 2000,
                    "vram_gb": 4.0,
                    "model": "YuanGZA/Orpheus-GPT2-v0.8",
                    "type": "midi_generation"
                }
            ],
            "sparklines": {
                "util": {"min": 0, "max": 100, "current": 1, "peak": 1, "avg": 1, "stddev": 0, "spark": "▁▁▁▁▁▁▁▁▁▁"},
                "temp": {"min": 20, "max": 85, "current": 28, "peak": 28, "avg": 27.6, "stddev": 0.49, "delta": 0, "spark": "▁▁▁▁▁▁▁▁▁▁"},
                "power": {"min": 0, "max": 120, "current": 33, "peak": 33.1, "avg": 32.2, "stddev": 0.41, "spark": "▂▂▂▂▂▂▂▂▂▂"},
                "vram": {"min": 0, "max": 96, "current": 49.5, "peak": 49.5, "avg": 49.5, "stddev": 0, "spark": "▄▄▄▄▄▄▄▄▄▄"},
                "window_seconds": 60,
                "sample_count": 60
            },
            "trends": {
                "temp": "stable",
                "power": "stable",
                "activity": "steady"
            },
            "notes": []
        }"#;

        let status: ObserverStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.health, "good");
        assert_eq!(status.gpu.util_pct, 1.0);
        assert_eq!(status.gpu.vram_used_gb, 49.5);
        assert_eq!(status.gpu.temp_c, 28.0);
        assert_eq!(status.services.len(), 1);
        assert_eq!(status.services[0].name, "orpheus-base");
        assert_eq!(status.sparklines.util.spark, "▁▁▁▁▁▁▁▁▁▁");
    }
}
