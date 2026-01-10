# 01: Cap'n Proto Schemas

**Files:** `crates/hooteproto/schemas/*.capnp`
**Focus:** Schema definitions only
**Dependencies:** None
**Unblocks:** 02-codegen-setup

---

## Task

Create all Cap'n Proto schema files by translating existing types from `crates/hooteproto/src/lib.rs` and `crates/hooteproto/src/garden.rs`.

**Deliverables:**
1. `schemas/common.capnp` — Timestamp, identifiers, common types
2. `schemas/envelope.capnp` — Message envelope
3. `schemas/tools.capnp` — All Payload variants as tool requests
4. `schemas/garden.capnp` — Garden protocol types
5. `schemas/broadcast.capnp` — Broadcast events
6. `schemas/jobs.capnp` — Job system types

**Definition of Done:**
```bash
# Schemas should parse (we'll compile in task 02)
capnp compile -o- schemas/*.capnp
```

## Out of Scope

- ❌ build.rs setup — that's task 02
- ❌ Rust helpers — that's task 04
- ❌ Migration code — that's tasks 05-06

Focus ONLY on schema definitions.

---

## Cap'n Proto Patterns

```capnp
@0xYYYYYYYYYYYYYYYY;  # Unique file ID (generate with: capnp id)

using Common = import "common.capnp";

struct MyStruct {
  field1 @0 :Text;
  field2 @1 :UInt64;
  optionalField @2 :Text;  # Optional by default
}

enum MyEnum {
  variant1 @0;
  variant2 @1;
}

struct WithUnion {
  union {
    variant1 @0 :Void;
    variant2 @1 :Text;
    variant3 @2 :MyStruct;
  }
}

struct WithList {
  items @0 :List(Text);
}
```

---

## Schema: common.capnp

```capnp
@0x...; # Generate unique ID

struct Timestamp {
  nanos @0 :UInt64;  # Nanoseconds since UNIX epoch
}

struct Uuid {
  low @0 :UInt64;
  high @1 :UInt64;
}

struct Error {
  code @0 :Text;
  message @1 :Text;
  details @2 :Text;  # JSON string for flexibility
}

enum JobStatus {
  pending @0;
  running @1;
  complete @2;
  failed @3;
  cancelled @4;
}
```

---

## Schema: envelope.capnp

Translate from `Envelope` in lib.rs:

```capnp
@0x...;

using Common = import "common.capnp";

struct Envelope {
  id @0 :Common.Uuid;
  traceparent @1 :Text;
  payload @2 :Payload;
}

struct Payload {
  union {
    # Worker management
    register @0 :WorkerRegistration;
    ping @1 :Void;
    pong @2 :Pong;
    shutdown @3 :Shutdown;

    # Tools — reference tools.capnp
    toolRequest @4 :import "tools.capnp".ToolRequest;

    # Responses
    success @5 :Success;
    error @6 :Common.Error;

    # ... continue for all Payload variants
  }
}

struct WorkerRegistration {
  workerId @0 :Common.Uuid;
  workerType @1 :WorkerType;
  workerName @2 :Text;
  capabilities @3 :List(Text);
  hostname @4 :Text;
  version @5 :Text;
}

enum WorkerType {
  luanette @0;
  hootenanny @1;
  chaosgarden @2;
}

struct Pong {
  workerId @0 :Common.Uuid;
  uptimeSecs @1 :UInt64;
}

struct ShutdownRequest {
  reason @0 :Text;
}

struct Success {
  result @0 :Text;  # JSON string
}
```

---

## Schema: tools.capnp

Translate each tool from the `Payload` enum. Example structure:

```capnp
@0x...;

using Common = import "common.capnp";

struct ArtifactMetadata {
  variationSetId @0 :Text;
  parentId @1 :Text;
  tags @2 :List(Text);
  creator @3 :Text;
}

struct ToolRequest {
  union {
    # CAS
    casStore @0 :CasStore;
    casInspect @1 :CasInspect;
    casGet @2 :CasGet;
    casUploadFile @3 :CasUploadFile;

    # Orpheus
    orpheusGenerate @4 :OrpheusGenerate;
    orpheusGenerateSeeded @5 :OrpheusGenerateSeeded;
    orpheusContinue @6 :OrpheusContinue;
    orpheusBridge @7 :OrpheusBridge;
    orpheusLoops @8 :OrpheusLoops;
    orpheusClassify @9 :OrpheusClassify;

    # ABC
    abcParse @10 :AbcParse;
    abcToMidi @11 :AbcToMidi;
    abcValidate @12 :AbcValidate;
    abcTranspose @13 :AbcTranspose;

    # ... continue for all tools
  }
}

struct CasStore {
  data @0 :Data;
  mimeType @1 :Text;
}

struct CasInspect {
  hash @0 :Text;
}

struct OrpheusGenerate {
  model @0 :Text;
  temperature @1 :Float32;
  topP @2 :Float32;
  maxTokens @3 :UInt32;
  numVariations @4 :UInt32;
  metadata @5 :ArtifactMetadata;
}

# Continue for all tool types...
```

---

## Schema: garden.capnp

Translate from `garden.rs`:

```capnp
@0x...;

using Common = import "common.capnp";

struct Beat {
  value @0 :Float64;
}

struct MessageHeader {
  msgId @0 :Common.Uuid;
  session @1 :Common.Uuid;
  msgType @2 :Text;
  version @3 :Text;
  timestamp @4 :Common.Timestamp;
}

struct ShellRequest {
  union {
    # Region operations
    createRegion @0 :CreateRegion;
    deleteRegion @1 :DeleteRegion;
    moveRegion @2 :MoveRegion;

    # Playback control
    play @3 :Void;
    pause @4 :Void;
    stop @5 :Void;
    seek @6 :Beat;
    setTempo @7 :Float64;

    # ... continue
  }
}

struct CreateRegion {
  position @0 :Beat;
  duration @1 :Beat;
  behavior @2 :Behavior;
}

struct Behavior {
  union {
    playContent @0 :Text;  # artifact_id
    latent @1 :Text;       # job_id
  }
}

# ... continue for ShellReply, IOPubEvent, etc.
```

---

## Schema: broadcast.capnp

Translate from `Broadcast` enum:

```capnp
@0x...;

using Common = import "common.capnp";

struct Broadcast {
  union {
    configUpdate @0 :ConfigUpdate;
    shutdown @1 :Shutdown;
    jobStateChanged @2 :JobStateChanged;
    progress @3 :Progress;
    artifactCreated @4 :ArtifactCreated;
    beatTick @5 :BeatTick;
    log @6 :Log;
    # ... etc
  }
}

struct BeatTick {
  timestamp @0 :Common.Timestamp;
  beat @1 :UInt64;
  positionBeats @2 :Float64;
  tempoBpm @3 :Float64;
}

# ... etc
```

---

## Schema: jobs.capnp

```capnp
@0x...;

using Common = import "common.capnp";

struct JobInfo {
  jobId @0 :Text;
  status @1 :Common.JobStatus;
  source @2 :Text;
  result @3 :Text;  # JSON
  error @4 :Text;
  createdAt @5 :UInt64;
  startedAt @6 :UInt64;
  completedAt @7 :UInt64;
}

struct JobStoreStats {
  total @0 :UInt32;
  pending @1 :UInt32;
  running @2 :UInt32;
  completed @3 :UInt32;
  failed @4 :UInt32;
  cancelled @5 :UInt32;
}
```

---

## Reference: Existing Types

Read these files to get the complete type definitions:
- `crates/hooteproto/src/lib.rs` — Payload enum, JobInfo, Broadcast
- `crates/hooteproto/src/garden.rs` — Garden protocol types

---

## Acceptance Criteria

- [ ] All 6 schema files created
- [ ] Each schema has unique `@0x...` file ID
- [ ] Field numbers are sequential and stable
- [ ] All Payload variants have corresponding capnp types
- [ ] Schemas parse without errors
