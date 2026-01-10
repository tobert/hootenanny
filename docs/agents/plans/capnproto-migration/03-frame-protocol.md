# 03: Frame Protocol Update

**Files:** `crates/hooteproto/src/frame.rs`
**Focus:** HootFrame ContentType handling
**Dependencies:** 02-codegen-setup
**Unblocks:** 05-hootenanny, 06-chaosgarden

---

## Task

Update HootFrame to handle Cap'n Proto content type and provide methods for capnp serialization/deserialization.

**Deliverables:**
1. `ContentType::CapnProto` variant (reuse MsgPack's 0x0001 value)
2. Methods to create/read capnp frames
3. Tests for roundtrip

**Definition of Done:**
```bash
cargo test -p hooteproto frame
```

## Out of Scope

- ❌ Migrating callers — that's tasks 05-06

**Note:** Remove old MsgPack methods entirely. No backwards compat.

---

## ContentType Changes

Just rename MsgPack → CapnProto. No backwards compat needed.

```rust
#[repr(u16)]
pub enum ContentType {
    Empty = 0x0000,
    CapnProto = 0x0001,
    RawBinary = 0x0002,
    Json = 0x0003,
}
```

---

## New Frame Methods

```rust
impl HootFrame {
    /// Create a request frame with Cap'n Proto payload
    pub fn request_capnp(
        service: &str,
        message: &capnp::message::Builder<capnp::message::HeapAllocator>,
    ) -> Self {
        let words = capnp::serialize::write_message_to_words(message);
        let bytes = capnp::Word::words_to_bytes(&words);
        Self {
            command: Command::Request,
            content_type: ContentType::CapnProto,
            request_id: Uuid::new_v4(),
            service: service.to_string(),
            traceparent: None,
            body: Bytes::copy_from_slice(bytes),
        }
    }

    /// Read Cap'n Proto payload (zero-copy from body)
    pub fn read_capnp(&self) -> Result<capnp::message::Reader<capnp::serialize::SliceSegments>, FrameError> {
        if self.content_type != ContentType::CapnProto {
            return Err(FrameError::ContentTypeMismatch {
                expected: ContentType::CapnProto,
                actual: self.content_type,
            });
        }
        let reader = capnp::serialize::read_message_from_flat_slice(
            &mut self.body.as_ref(),
            capnp::message::ReaderOptions::default(),
        )?;
        Ok(reader)
    }

    /// Create reply with Cap'n Proto payload
    pub fn reply_capnp(
        request_id: Uuid,
        message: &capnp::message::Builder<capnp::message::HeapAllocator>,
    ) -> Self {
        let words = capnp::serialize::write_message_to_words(message);
        let bytes = capnp::Word::words_to_bytes(&words);
        Self {
            command: Command::Reply,
            content_type: ContentType::CapnProto,
            request_id,
            service: String::new(),
            traceparent: None,
            body: Bytes::copy_from_slice(bytes),
        }
    }
}
```

---

## Error Handling

Add to `FrameError`:

```rust
#[error("Cap'n Proto error: {0}")]
CapnProto(#[from] capnp::Error),
```

---

## Tests

```rust
#[test]
fn capnp_roundtrip() {
    use crate::capnp_gen::common_capnp;

    // Build a message
    let mut message = capnp::message::Builder::new_default();
    {
        let mut timestamp = message.init_root::<common_capnp::timestamp::Builder>();
        timestamp.set_nanos(1234567890);
    }

    // Create frame
    let frame = HootFrame::request_capnp("test", &message);
    assert_eq!(frame.content_type, ContentType::CapnProto);

    // Roundtrip through wire format
    let frames = frame.to_frames();
    let parsed = HootFrame::from_frames(&frames).unwrap();

    // Read back
    let reader = parsed.read_capnp().unwrap();
    let ts = reader.get_root::<common_capnp::timestamp::Reader>().unwrap();
    assert_eq!(ts.get_nanos(), 1234567890);
}
```

---

## Also Update: ZMQ Client Trait

If there's a ZMQ client abstraction that callers use, add capnp method:

```rust
impl ZmqClient {
    pub async fn request_capnp(
        &self,
        service: &str,
        message: &capnp::message::Builder<capnp::message::HeapAllocator>,
    ) -> Result<HootFrame> {
        let frame = HootFrame::request_capnp(service, message);
        self.send_and_receive(frame).await
    }
}
```

Or just have callers construct frames directly — simpler.

---

## Acceptance Criteria

- [ ] `ContentType::CapnProto` exists (renamed from MsgPack)
- [ ] `request_capnp()` creates valid frames
- [ ] `reply_capnp()` creates valid frames
- [ ] `read_capnp()` returns zero-copy reader
- [ ] Old MsgPack methods removed
- [ ] Roundtrip test passes
