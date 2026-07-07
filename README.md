# NowledgeMem Rust SDK

Rust client library for the [Nowledge Mem](https://mem.nowledge.co) REST API.

The SDK is generated from the Nowledge Mem OpenAPI document and wrapped with
Rust-native configuration helpers that match the Go SDK behavior.

OpenAPI source: <https://mem.nowledge.co/docs/refs/openapi.json>

## Installation

Requires Rust 1.88 or newer.

```toml
[dependencies]
nowledgemem = { git = "https://github.com/lib-x/nowledgemem-rs" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Quick Start

```rust
use nowledgemem::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;

    let health = client
        .api()
        .health_check_health_get()
        .send()
        .await?
        .into_inner();

    println!("status: {}", health.status);
    Ok(())
}
```

## Configuration

`Client::new()` targets `http://127.0.0.1:14242`.

```rust
use std::time::Duration;
use nowledgemem::Client;

let local = Client::new()?;

let remote = Client::remote(
    "https://mem.example.com",
    std::env::var("NMEM_API_KEY")?,
)?;

let from_env = Client::from_env()?;

let custom = Client::builder()
    .base_url("http://192.168.1.100:14242")
    .timeout(Duration::from_secs(60))
    .api_key("nmem_xxxx")
    .build()?;
```

Environment variables:

- `NMEM_API_URL`: backend API URL.
- `NMEM_API_KEY`: API key for LAN or remote deployments.

`Client::from_config()` reads `~/.nowledge-mem/config.json`, with environment
variables overriding file values.

`api_key()` sends both supported header forms:

- `Authorization: Bearer nmem_xxxx`
- `X-NMEM-API-Key: nmem_xxxx`

`api_key_query()` sends `nmem_api_key=nmem_xxxx` on every request for proxies
that strip headers.

## Calling Endpoints

All non-multipart OpenAPI operations are exposed through `client.api()` using
builder-style methods.

```rust
let memories = client
    .api()
    .list_memories_memories_get()
    .limit(10)
    .send()
    .await?
    .into_inner();
```

Request and response types are available under `nowledgemem::types`.

## Multipart Uploads

`progenitor` does not generate multipart operations, so the SDK provides
hand-written methods for the three upload endpoints:

- `Client::import_data_upload`
- `Client::ingest_source_file`
- `Client::ingest_source_folder`

```rust
use nowledgemem::{Client, SourceFileUploadRequest};
use reqwest::multipart::Part;

let client = Client::new()?;
let mut request = SourceFileUploadRequest::new(
    Part::bytes(b"# Note".to_vec()).file_name("note.md"),
);
request.space_id = Some("work".to_string());

let source = client.ingest_source_file(request).await?.into_inner();
println!("source: {}", source.source_id);
```

## OpenAPI Generation Notes

The original OpenAPI document is stored at
`openapi/nowledge-mem.openapi.json`. The build script keeps that file as the
source of truth and applies minimal generation-only normalization for:

- OpenAPI 3.1 to 3.0 compatibility.
- `anyOf + null` and `type: [..., "null"]` nullable schemas.
- inline enum title disambiguation.
- `multipart/form-data` operations, which are implemented by hand.
- operations with multiple structured error response bodies.

## License

MIT
