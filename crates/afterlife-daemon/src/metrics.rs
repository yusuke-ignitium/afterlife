use std::{
    collections::{HashMap, VecDeque},
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use serde_json::{Value, json};

use crate::auth::now_ms;

const MAX_LATENCY_SAMPLES: usize = 4096;
const MAX_TOOL_SAMPLES: usize = 1024;

#[derive(Default)]
struct ToolStats {
    calls: u64,
    errors: u64,
    conflicts: u64,
    forbidden: u64,
    total_us: u64,
    max_us: u64,
    samples: VecDeque<u64>,
}

impl ToolStats {
    fn record(&mut self, latency_us: u64, outcome: &str) {
        self.calls += 1;
        self.total_us = self.total_us.saturating_add(latency_us);
        if latency_us > self.max_us {
            self.max_us = latency_us;
        }
        match outcome {
            "conflict" => {
                self.errors += 1;
                self.conflicts += 1;
            }
            "forbidden" => {
                self.errors += 1;
                self.forbidden += 1;
            }
            "error" => {
                self.errors += 1;
            }
            _ => {}
        }
        if self.samples.len() >= MAX_TOOL_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(latency_us);
    }

    fn snapshot(&self) -> Value {
        let avg_us = self.total_us.checked_div(self.calls).unwrap_or(0);
        let (p50, p95, p99) = percentiles(&self.samples);
        json!({
            "calls": self.calls,
            "errors": self.errors,
            "conflicts": self.conflicts,
            "forbidden": self.forbidden,
            "avg_us": avg_us,
            "p50_us": p50,
            "p95_us": p95,
            "p99_us": p99,
            "max_us": self.max_us,
        })
    }
}

type RuntimeMetricsProvider = Arc<dyn Fn() -> Value + Send + Sync>;

struct MetricsInner {
    latencies_us: VecDeque<u64>,
    max_latency_us: u64,
    tools: HashMap<String, ToolStats>,
    log_writer: Option<BufWriter<File>>,
    runtime_provider: Option<RuntimeMetricsProvider>,
}

pub struct DaemonMetrics {
    started_at: Instant,
    log_path: Option<PathBuf>,
    log_stderr: bool,
    pub total_requests: AtomicU64,
    pub in_flight: AtomicU64,
    pub status_2xx: AtomicU64,
    pub status_401: AtomicU64,
    pub status_403: AtomicU64,
    pub status_409: AtomicU64,
    pub status_4xx_other: AtomicU64,
    pub status_5xx: AtomicU64,
    pub mcp_notifications: AtomicU64,
    inner: Mutex<MetricsInner>,
}

impl DaemonMetrics {
    pub fn new(log_dir: Option<&Path>) -> Arc<Self> {
        let (log_path, log_writer) = match log_dir {
            Some(dir) => {
                let _ = std::fs::create_dir_all(dir);
                let path = dir.join("requests.jsonl");
                let writer = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .ok()
                    .map(BufWriter::new);
                (Some(path), writer)
            }
            None => (None, None),
        };
        let log_stderr = std::env::var("AFTERLIFE_LOG_STDERR")
            .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
            .unwrap_or(false);

        Arc::new(Self {
            started_at: Instant::now(),
            log_path,
            log_stderr,
            total_requests: AtomicU64::new(0),
            in_flight: AtomicU64::new(0),
            status_2xx: AtomicU64::new(0),
            status_401: AtomicU64::new(0),
            status_403: AtomicU64::new(0),
            status_409: AtomicU64::new(0),
            status_4xx_other: AtomicU64::new(0),
            status_5xx: AtomicU64::new(0),
            mcp_notifications: AtomicU64::new(0),
            inner: Mutex::new(MetricsInner {
                latencies_us: VecDeque::with_capacity(MAX_LATENCY_SAMPLES),
                max_latency_us: 0,
                tools: HashMap::new(),
                log_writer,
                runtime_provider: None,
            }),
        })
    }

    pub fn set_runtime_provider(&self, provider: RuntimeMetricsProvider) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.runtime_provider = Some(provider);
    }

    pub fn log_path(&self) -> Option<&Path> {
        self.log_path.as_deref()
    }

    pub fn record_http(&self, method: &str, path: &str, status: u16, latency_us: u64) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        match status {
            200..=299 => self.status_2xx.fetch_add(1, Ordering::Relaxed),
            401 => self.status_401.fetch_add(1, Ordering::Relaxed),
            403 => self.status_403.fetch_add(1, Ordering::Relaxed),
            409 => self.status_409.fetch_add(1, Ordering::Relaxed),
            400..=499 => self.status_4xx_other.fetch_add(1, Ordering::Relaxed),
            _ => self.status_5xx.fetch_add(1, Ordering::Relaxed),
        };

        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if g.latencies_us.len() >= MAX_LATENCY_SAMPLES {
            g.latencies_us.pop_front();
        }
        g.latencies_us.push_back(latency_us);
        if latency_us > g.max_latency_us {
            g.max_latency_us = latency_us;
        }

        let line = json!({
            "ts_ms": now_ms(),
            "kind": "http",
            "method": method,
            "path": path,
            "status": status,
            "latency_us": latency_us,
        });
        Self::write_log_line(&mut g.log_writer, self.log_stderr, &line);
    }

    pub fn record_mcp_call(
        &self,
        actor_label: &str,
        mcp_method: &str,
        tool: Option<&str>,
        outcome: &str,
        latency_us: u64,
        detail: Option<&str>,
    ) {
        if mcp_method.starts_with("notifications/") {
            self.mcp_notifications.fetch_add(1, Ordering::Relaxed);
        }
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(tool_name) = tool {
            g.tools
                .entry(tool_name.to_string())
                .or_default()
                .record(latency_us, outcome);
        }
        let line = json!({
            "ts_ms": now_ms(),
            "kind": "mcp",
            "actor": actor_label,
            "mcp_method": mcp_method,
            "tool": tool,
            "outcome": outcome,
            "latency_us": latency_us,
            "detail": detail,
        });
        Self::write_log_line(&mut g.log_writer, self.log_stderr, &line);
    }

    pub fn reset(&self) {
        self.total_requests.store(0, Ordering::Relaxed);
        self.status_2xx.store(0, Ordering::Relaxed);
        self.status_401.store(0, Ordering::Relaxed);
        self.status_403.store(0, Ordering::Relaxed);
        self.status_409.store(0, Ordering::Relaxed);
        self.status_4xx_other.store(0, Ordering::Relaxed);
        self.status_5xx.store(0, Ordering::Relaxed);
        self.mcp_notifications.store(0, Ordering::Relaxed);
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.latencies_us.clear();
        g.max_latency_us = 0;
        g.tools.clear();
    }

    pub fn snapshot(&self, telemetry_dir: Option<String>) -> Value {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let (p50, p95, p99) = percentiles(&g.latencies_us);
        let mut tools_map = serde_json::Map::new();
        let mut keys: Vec<&String> = g.tools.keys().collect();
        keys.sort();
        for k in keys {
            tools_map.insert(k.clone(), g.tools[k].snapshot());
        }
        let uptime_secs = self.started_at.elapsed().as_secs_f64();
        let total = self.total_requests.load(Ordering::Relaxed);
        let rps = if uptime_secs > 0.001 {
            (total as f64 / uptime_secs * 100.0).round() / 100.0
        } else {
            0.0
        };
        let latency_samples = g.latencies_us.len();
        let max_latency_us = g.max_latency_us;
        let provider = g.runtime_provider.clone();
        drop(g);
        let desktop_runtime = provider.map(|f| f());
        json!({
            "uptime_secs": (uptime_secs * 100.0).round() / 100.0,
            "total_requests": total,
            "avg_rps": rps,
            "in_flight": self.in_flight.load(Ordering::Relaxed),
            "status_counts": {
                "2xx": self.status_2xx.load(Ordering::Relaxed),
                "401": self.status_401.load(Ordering::Relaxed),
                "403": self.status_403.load(Ordering::Relaxed),
                "409": self.status_409.load(Ordering::Relaxed),
                "4xx_other": self.status_4xx_other.load(Ordering::Relaxed),
                "5xx": self.status_5xx.load(Ordering::Relaxed),
            },
            "latency_us": {
                "samples": latency_samples,
                "p50": p50,
                "p95": p95,
                "p99": p99,
                "max": max_latency_us,
            },
            "mcp_notifications": self.mcp_notifications.load(Ordering::Relaxed),
            "mcp_tools": tools_map,
            "desktop_runtime": desktop_runtime,
            "log_path": self.log_path.as_ref().map(|p| p.display().to_string()),
            "telemetry_dir": telemetry_dir,
        })
    }

    fn write_log_line(writer: &mut Option<BufWriter<File>>, log_stderr: bool, line: &Value) {
        if let Ok(encoded) = serde_json::to_string(line) {
            if let Some(w) = writer.as_mut() {
                let _ = writeln!(w, "{encoded}");
                let _ = w.flush();
            }
            if log_stderr {
                eprintln!("[afterlifed] {encoded}");
            }
        }
    }
}

fn percentiles(samples: &VecDeque<u64>) -> (u64, u64, u64) {
    if samples.is_empty() {
        return (0, 0, 0);
    }
    let mut sorted: Vec<u64> = samples.iter().copied().collect();
    sorted.sort_unstable();
    let pick = |pct: f64| -> u64 {
        let idx = (((sorted.len() - 1) as f64) * pct).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    };
    (pick(0.50), pick(0.95), pick(0.99))
}
