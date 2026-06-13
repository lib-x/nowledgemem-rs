//! Rust client library for the Nowledge Mem REST API.
//!
//! The low-level OpenAPI client is available under [`api`]. The crate-level
//! constructors add Nowledge Mem defaults for base URLs, API keys, and the
//! shared local client config used by the Go SDK and `nmem` tooling.

mod client;
mod multipart;

pub mod api {
    #![allow(clippy::all)]
    #![allow(dead_code)]
    #![allow(missing_docs)]
    include!(concat!(env!("OUT_DIR"), "/codegen.rs"));
}

pub use client::{
    Client, ClientConfig, ClientError, ClientState, DEFAULT_BASE_URL, ENV_API_KEY, ENV_API_URL,
    apply_request_options,
};
pub use multipart::{
    DataImportUploadRequest, FolderUploadFile, SourceFileUploadRequest, SourceFolderUploadRequest,
};

pub use api::types;
