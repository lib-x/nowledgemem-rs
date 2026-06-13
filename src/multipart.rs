use reqwest::header::{ACCEPT, HeaderName, HeaderValue};
use reqwest::multipart::{Form, Part};
use serde::de::DeserializeOwned;

use crate::{Client, api, types};
use progenitor_client::ClientInfo;

/// Multipart request for `POST /data/import/upload`.
pub struct DataImportUploadRequest {
    /// ZIP file part.
    pub file: Part,
    /// Import conflict behavior: `merge`, `skip`, or `overwrite`.
    pub mode: Option<String>,
    pub include_memories: Option<bool>,
    pub include_threads: Option<bool>,
    pub include_messages: Option<bool>,
    pub include_entities: Option<bool>,
    pub include_labels: Option<bool>,
    pub include_sources: Option<bool>,
    pub include_communities: Option<bool>,
    pub include_skills: Option<bool>,
    pub include_edges: Option<bool>,
    pub include_working_memory: Option<bool>,
    pub include_working_memory_archive: Option<bool>,
    pub include_source_files: Option<bool>,
}

impl DataImportUploadRequest {
    /// Create a data import upload request from a multipart file part.
    pub fn new(file: Part) -> Self {
        Self {
            file,
            mode: None,
            include_memories: None,
            include_threads: None,
            include_messages: None,
            include_entities: None,
            include_labels: None,
            include_sources: None,
            include_communities: None,
            include_skills: None,
            include_edges: None,
            include_working_memory: None,
            include_working_memory_archive: None,
            include_source_files: None,
        }
    }
}

/// Multipart request for `POST /sources/ingest/file`.
pub struct SourceFileUploadRequest {
    /// File part.
    pub file: Part,
    pub user_comment: Option<String>,
    /// Comma-separated labels, matching the server's multipart contract.
    pub labels: Option<String>,
    /// JSON-encoded metadata string, matching the server's multipart contract.
    pub metadata: Option<String>,
    pub space_id: Option<String>,
}

impl SourceFileUploadRequest {
    /// Create a source file upload request from a multipart file part.
    pub fn new(file: Part) -> Self {
        Self {
            file,
            user_comment: None,
            labels: None,
            metadata: None,
            space_id: None,
        }
    }
}

/// A single file in a folder upload.
pub struct FolderUploadFile {
    /// File part. Set a filename on the part when the server should preserve it.
    pub file: Part,
}

impl FolderUploadFile {
    /// Create a folder upload file from a multipart file part.
    pub fn new(file: Part) -> Self {
        Self { file }
    }
}

/// Multipart request for `POST /sources/ingest/folder-upload`.
pub struct SourceFolderUploadRequest {
    pub files: Vec<FolderUploadFile>,
    pub folder_name: String,
    /// JSON-encoded manifest of folder-relative paths.
    pub file_manifest: Option<String>,
    pub user_comment: Option<String>,
    /// Comma-separated labels, matching the server's multipart contract.
    pub labels: Option<String>,
    pub space_id: Option<String>,
    pub emit_feed_event: Option<bool>,
    /// JSON-encoded accumulated totals.
    pub accumulated_totals: Option<String>,
}

impl SourceFolderUploadRequest {
    /// Create a folder upload request.
    pub fn new(folder_name: impl Into<String>, files: Vec<FolderUploadFile>) -> Self {
        Self {
            files,
            folder_name: folder_name.into(),
            file_manifest: None,
            user_comment: None,
            labels: None,
            space_id: None,
            emit_feed_event: None,
            accumulated_totals: None,
        }
    }
}

impl Client {
    /// Import data from an uploaded ZIP file.
    pub async fn import_data_upload(
        &self,
        request: DataImportUploadRequest,
    ) -> Result<
        api::ResponseValue<types::DataImportStartResponse>,
        api::Error<types::HttpValidationError>,
    > {
        let mut form = Form::new().part("file", request.file);
        form = optional_text(form, "mode", request.mode);
        form = optional_bool(form, "include_memories", request.include_memories);
        form = optional_bool(form, "include_threads", request.include_threads);
        form = optional_bool(form, "include_messages", request.include_messages);
        form = optional_bool(form, "include_entities", request.include_entities);
        form = optional_bool(form, "include_labels", request.include_labels);
        form = optional_bool(form, "include_sources", request.include_sources);
        form = optional_bool(form, "include_communities", request.include_communities);
        form = optional_bool(form, "include_skills", request.include_skills);
        form = optional_bool(form, "include_edges", request.include_edges);
        form = optional_bool(
            form,
            "include_working_memory",
            request.include_working_memory,
        );
        form = optional_bool(
            form,
            "include_working_memory_archive",
            request.include_working_memory_archive,
        );
        form = optional_bool(form, "include_source_files", request.include_source_files);
        self.send_multipart("/data/import/upload", form).await
    }

    /// Ingest one uploaded file into the library.
    pub async fn ingest_source_file(
        &self,
        request: SourceFileUploadRequest,
    ) -> Result<api::ResponseValue<types::IngestResponse>, api::Error<types::HttpValidationError>>
    {
        let mut form = Form::new().part("file", request.file);
        form = optional_text(form, "user_comment", request.user_comment);
        form = optional_text(form, "labels", request.labels);
        form = optional_text(form, "metadata", request.metadata);
        form = optional_text(form, "space_id", request.space_id);
        self.send_multipart("/sources/ingest/file", form).await
    }

    /// Ingest uploaded folder files while preserving relative paths via a manifest.
    pub async fn ingest_source_folder(
        &self,
        request: SourceFolderUploadRequest,
    ) -> Result<
        api::ResponseValue<types::BatchIngestResponse>,
        api::Error<types::HttpValidationError>,
    > {
        if request.folder_name.is_empty() {
            return Err(api::Error::InvalidRequest(
                "folder_name is required".to_string(),
            ));
        }
        if request.files.is_empty() {
            return Err(api::Error::InvalidRequest(
                "at least one file is required".to_string(),
            ));
        }

        let mut form = Form::new().text("folder_name", request.folder_name);
        for file in request.files {
            form = form.part("files", file.file);
        }
        form = optional_text(form, "file_manifest", request.file_manifest);
        form = optional_text(form, "user_comment", request.user_comment);
        form = optional_text(form, "labels", request.labels);
        form = optional_text(form, "space_id", request.space_id);
        form = optional_bool(form, "emit_feed_event", request.emit_feed_event);
        form = optional_text(form, "accumulated_totals", request.accumulated_totals);
        self.send_multipart("/sources/ingest/folder-upload", form)
            .await
    }

    async fn send_multipart<T>(
        &self,
        path: &str,
        form: Form,
    ) -> Result<api::ResponseValue<T>, api::Error<types::HttpValidationError>>
    where
        T: DeserializeOwned,
    {
        let url = format!("{}{}", self.api().baseurl.trim_end_matches('/'), path);
        let mut request = self
            .api()
            .client
            .post(url)
            .header(ACCEPT, HeaderValue::from_static("application/json"))
            .header(
                HeaderName::from_static("api-version"),
                HeaderValue::from_static(api::Client::api_version()),
            )
            .multipart(form)
            .build()?;

        crate::apply_request_options(&self.api().inner, &mut request)
            .await
            .map_err(|error| api::Error::Custom(error.to_string()))?;

        let response = self.api().client.execute(request).await?;
        match response.status().as_u16() {
            200 => api::ResponseValue::from_response(response).await,
            422 => Err(api::Error::ErrorResponse(
                api::ResponseValue::from_response(response).await?,
            )),
            _ => Err(api::Error::UnexpectedResponse(response)),
        }
    }
}

fn optional_text(mut form: Form, name: &'static str, value: Option<String>) -> Form {
    if let Some(value) = value
        && !value.is_empty()
    {
        form = form.text(name, value);
    }
    form
}

fn optional_bool(form: Form, name: &'static str, value: Option<bool>) -> Form {
    optional_text(form, name, value.map(|value| value.to_string()))
}
