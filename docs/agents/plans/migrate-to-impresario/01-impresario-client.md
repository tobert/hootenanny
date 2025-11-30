# Task 01: Impresario Client

Rust HTTP client for impresario.

## Location

`crates/hootenanny/src/impresario.rs`

## Interface

```rust
pub struct ImpresarioClient {
    base_url: String,
    client: reqwest::Client,
}

impl ImpresarioClient {
    pub fn new(base_url: &str) -> Self;
    pub async fn submit(&self, service: &str, params: Value) -> Result<Job>;
    pub async fn get(&self, job_id: &str) -> Result<Job>;
    pub async fn cancel(&self, job_id: &str) -> Result<Job>;
    pub async fn health(&self) -> Result<Health>;
}

pub struct Job {
    pub id: String,
    pub status: String,  // queued|running|complete|failed|cancelled
    pub result: Option<Value>,
    pub error: Option<String>,
}
```

## Usage

```rust
let imp = ImpresarioClient::new("http://localhost:1337");
let job = imp.submit("orpheus-base", json!({"temperature": 1.0})).await?;
// poll...
let done = imp.get(&job.id).await?;
```

## Done When

- [ ] Can submit jobs
- [ ] Can poll status
- [ ] Propagates traceparent header
