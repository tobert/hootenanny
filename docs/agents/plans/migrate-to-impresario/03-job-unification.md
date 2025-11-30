# Task 03: Job Unification

Use impresario as the job store. Hootenanny's in-memory JobStore becomes a thin wrapper.

## Current

```rust
// Hootenanny's JobStore - in-memory
pub struct JobStore {
    jobs: HashMap<String, JobInfo>,
    handles: HashMap<String, JoinHandle<()>>,
}
```

## After

```rust
// Thin wrapper that delegates to impresario
pub struct JobStore {
    impresario: Arc<ImpresarioClient>,
    // Local tracking only for artifact creation on completion
    pending_artifacts: HashMap<String, ArtifactMeta>,
}

impl JobStore {
    pub async fn get(&self, id: &str) -> Result<JobInfo> {
        let job = self.impresario.get(id).await?;
        Ok(job.into())
    }

    pub async fn list(&self) -> Result<Vec<JobInfo>> {
        // Could add /jobs?service=orpheus-base etc.
        // Or just track submitted job IDs locally
    }
}
```

## Poll Tool

```rust
async fn poll(&self, req: PollRequest) -> Result<CallToolResult> {
    let timeout = Duration::from_millis(req.timeout_ms.min(10_000));
    let start = Instant::now();

    loop {
        let mut done = vec![];
        let mut pending = vec![];

        for id in &req.job_ids {
            let job = self.impresario.get(id).await?;
            match job.status.as_str() {
                "complete" | "failed" | "cancelled" => done.push(job),
                _ => pending.push(id.clone()),
            }
        }

        if !done.is_empty() || start.elapsed() >= timeout {
            // Process completions (CAS storage, artifacts)
            for job in &done {
                self.process_completion(job).await?;
            }
            return Ok(json!({"completed": done, "pending": pending}));
        }

        sleep(Duration::from_millis(500)).await;
    }
}
```

## Artifact Creation

On job completion, hootenanny still:
1. Fetches MIDI from result (base64)
2. Stores in CAS
3. Creates artifact metadata

```rust
async fn process_completion(&self, job: &Job) -> Result<()> {
    if job.status != "complete" { return Ok(()); }

    let result = job.result.as_ref().ok_or("no result")?;
    let variations = result["variations"].as_array();

    for var in variations.unwrap_or(&vec![]) {
        let midi_b64 = var["midi_base64"].as_str().unwrap();
        let bytes = base64::decode(midi_b64)?;
        let hash = self.cas.store(&bytes, "audio/midi").await?;

        self.artifacts.create(/* ... */).await?;
    }
    Ok(())
}
```

## Done When

- [ ] `get_job_status` uses impresario
- [ ] `poll` uses impresario
- [ ] `list_jobs` works (or removed)
- [ ] Completions trigger CAS + artifact creation
- [ ] In-memory job tracking deleted
