use httpmock::prelude::*;
use nowledgemem::{
    Client, DataImportUploadRequest, FolderUploadFile, SourceFileUploadRequest,
    SourceFolderUploadRequest,
};
use reqwest::multipart::Part;
use serde_json::json;

#[tokio::test]
async fn generated_requests_send_auth_headers_and_query_key() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/health")
                .header("authorization", "Bearer nmem_header")
                .header("x-nmem-api-key", "nmem_header")
                .query_param("nmem_api_key", "nmem_query");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "status": "ok",
                    "version": "0.9.8",
                    "timestamp": "2026-06-13T00:00:00Z",
                    "database_connected": true,
                    "services_ready": true
                }));
        })
        .await;

    let client = Client::builder()
        .base_url(server.base_url())
        .api_key("Bearer nmem_header")
        .api_key_query("Bearer nmem_query")
        .build()
        .unwrap();

    let response = client
        .api()
        .health_check_health_get()
        .send()
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.status, "ok");
    mock.assert_async().await;
}

#[tokio::test]
async fn custom_http_client_still_applies_sdk_auth() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/health")
                .header("authorization", "Bearer nmem_custom")
                .header("x-nmem-api-key", "nmem_custom")
                .query_param("nmem_api_key", "nmem_query");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "status": "ok",
                    "version": "0.9.8",
                    "timestamp": "2026-06-13T00:00:00Z",
                    "database_connected": true,
                    "services_ready": true
                }));
        })
        .await;

    let http_client = reqwest::Client::builder().build().unwrap();
    let client = Client::builder()
        .base_url(server.base_url())
        .api_key("nmem_custom")
        .api_key_query("nmem_query")
        .http_client(http_client)
        .build()
        .unwrap();

    let response = client
        .api()
        .health_check_health_get()
        .send()
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.status, "ok");
    mock.assert_async().await;
}

#[tokio::test]
async fn source_file_upload_sends_multipart_form() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/sources/ingest/file")
                .header_includes("content-type", "multipart/form-data")
                .body_includes("name=\"file\"")
                .body_includes("filename=\"note.md\"")
                .body_includes("hello")
                .body_includes("name=\"space_id\"")
                .body_includes("work")
                .body_includes("name=\"labels\"")
                .body_includes("label_a,label_b");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "source_id": "src1",
                    "original_name": "note.md",
                    "lifecycle_state": "ready",
                    "is_duplicate": false
                }));
        })
        .await;

    let client = Client::builder()
        .base_url(server.base_url())
        .build()
        .unwrap();
    let mut request = SourceFileUploadRequest::new(
        Part::bytes(b"hello".to_vec())
            .file_name("note.md")
            .mime_str("text/markdown")
            .unwrap(),
    );
    request.space_id = Some("work".to_string());
    request.labels = Some("label_a,label_b".to_string());

    let response = client
        .ingest_source_file(request)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.source_id, "src1");
    mock.assert_async().await;
}

#[tokio::test]
async fn data_import_upload_sends_multipart_form() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/data/import/upload")
                .header_includes("content-type", "multipart/form-data")
                .body_includes("filename=\"backup.zip\"")
                .body_includes("zip-bytes")
                .body_includes("name=\"mode\"")
                .body_includes("merge")
                .body_includes("name=\"include_memories\"")
                .body_includes("true");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "job_id": "job1",
                    "status": "queued",
                    "message": "import started",
                    "success": true
                }));
        })
        .await;

    let client = Client::builder()
        .base_url(server.base_url())
        .build()
        .unwrap();
    let mut request =
        DataImportUploadRequest::new(Part::bytes(b"zip-bytes".to_vec()).file_name("backup.zip"));
    request.mode = Some("merge".to_string());
    request.include_memories = Some(true);

    let response = client
        .import_data_upload(request)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.job_id, "job1");
    mock.assert_async().await;
}

#[tokio::test]
async fn source_folder_upload_validates_and_sends_files() {
    let server = MockServer::start_async().await;
    let client = Client::builder()
        .base_url(server.base_url())
        .build()
        .unwrap();

    let empty = SourceFolderUploadRequest::new("docs", Vec::new());
    let err = client.ingest_source_folder(empty).await.unwrap_err();
    assert!(err.to_string().contains("at least one file is required"));

    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/sources/ingest/folder-upload")
                .header_includes("content-type", "multipart/form-data")
                .body_includes("filename=\"a.md\"")
                .body_includes("alpha")
                .body_includes("name=\"folder_name\"")
                .body_includes("docs")
                .body_includes("name=\"file_manifest\"")
                .body_includes("a.md");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "folder_name": "docs",
                    "total_ingested": 1,
                    "total_duplicates": 0,
                    "total_errors": 0,
                    "results": []
                }));
        })
        .await;

    let file = FolderUploadFile::new(Part::bytes(b"alpha".to_vec()).file_name("a.md"));
    let mut request = SourceFolderUploadRequest::new("docs", vec![file]);
    request.file_manifest = Some(r#"[{"relative_path":"a.md"}]"#.to_string());

    let response = client
        .ingest_source_folder(request)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.total_ingested, 1);
    mock.assert_async().await;
}

#[test]
fn base_url_rejects_query_parameters() {
    let err = Client::builder()
        .base_url("http://127.0.0.1:14242?x=1")
        .build()
        .unwrap_err();

    assert!(err.to_string().contains("must not contain query"));
}
