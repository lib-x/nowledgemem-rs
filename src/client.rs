use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::header::{AUTHORIZATION, HeaderValue};
use serde::{Deserialize, Serialize};
use url::Url;

/// Default Nowledge Mem API URL used by local deployments.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:14242";
/// Environment variable that overrides the Nowledge Mem API URL.
pub const ENV_API_URL: &str = "NMEM_API_URL";
/// Environment variable that provides the Nowledge Mem API key.
pub const ENV_API_KEY: &str = "NMEM_API_KEY";

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const HEADER_NMEM_API_KEY: &str = "X-NMEM-API-Key";
const QUERY_NMEM_API_KEY: &str = "nmem_api_key";

/// Shared local client configuration written by `nmem`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfig {
    /// Backend API URL, for example `http://127.0.0.1:14242`.
    #[serde(default)]
    pub api_url: String,
    /// Remote API key. Empty for local unauthenticated use.
    #[serde(default)]
    pub api_key: String,
}

/// Errors returned while constructing a Nowledge Mem client.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The configured API URL is not a valid absolute URL.
    #[error("invalid base URL {value:?}: {source}")]
    InvalidBaseUrl {
        /// The invalid URL value.
        value: String,
        /// Parser error from the `url` crate.
        #[source]
        source: url::ParseError,
    },
    /// Base URLs must not contain query parameters.
    #[error("base URL must not contain query parameters: {value:?}")]
    BaseUrlCannotContainQuery {
        /// The invalid URL value.
        value: String,
    },
    /// The provided API key was empty after trimming and Bearer normalization.
    #[error("{kind} cannot be empty")]
    EmptyApiKey {
        /// Which key option was empty.
        kind: &'static str,
    },
    /// A header value could not be created from the provided token.
    #[error("invalid {header} header value: {source}")]
    InvalidHeaderValue {
        /// Header name.
        header: &'static str,
        /// Header parsing error.
        #[source]
        source: reqwest::header::InvalidHeaderValue,
    },
    /// HTTP client construction failed.
    #[error("build HTTP client: {0}")]
    BuildHttpClient(#[source] reqwest::Error),
    /// Reading the shared local client config failed.
    #[error("read client config {path}: {source}")]
    ReadConfig {
        /// Path to the config file.
        path: PathBuf,
        /// Filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// Decoding the shared local client config failed.
    #[error("decode client config {path}: {source}")]
    DecodeConfig {
        /// Path to the config file.
        path: PathBuf,
        /// JSON decode error.
        #[source]
        source: serde_json::Error,
    },
}

/// High-level Nowledge Mem client.
#[derive(Clone, Debug)]
pub struct Client {
    inner: crate::api::Client,
    base_url: Url,
}

/// Extra request options carried by the generated OpenAPI client.
#[doc(hidden)]
#[derive(Clone, Debug, Default)]
pub struct ClientState {
    bearer_token: Option<HeaderValue>,
    header_api_key: Option<HeaderValue>,
    query_api_key: Option<String>,
}

#[doc(hidden)]
pub async fn apply_request_options(
    state: &ClientState,
    request: &mut reqwest::Request,
) -> Result<(), ClientError> {
    if let Some(value) = &state.bearer_token {
        request.headers_mut().insert(AUTHORIZATION, value.clone());
    }
    if let Some(value) = &state.header_api_key {
        request
            .headers_mut()
            .insert(HEADER_NMEM_API_KEY, value.clone());
    }
    if let Some(api_key) = &state.query_api_key {
        set_query_value(request.url_mut(), QUERY_NMEM_API_KEY, api_key);
    }
    Ok(())
}

impl Client {
    /// Create a client targeting the local Nowledge Mem API.
    pub fn new() -> Result<Self, ClientError> {
        Self::builder().build()
    }

    /// Start configuring a client.
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Create a client for a remote deployment.
    ///
    /// The API key is sent as both `Authorization: Bearer ...` and
    /// `X-NMEM-API-Key: ...`, matching the Go SDK and current server behavior.
    pub fn remote(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Result<Self, ClientError> {
        Self::builder().base_url(base_url).api_key(api_key).build()
    }

    /// Create a client from `NMEM_API_URL` and `NMEM_API_KEY`.
    pub fn from_env() -> Result<Self, ClientError> {
        Self::builder().apply_env().build()
    }

    /// Create a client from `~/.nowledge-mem/config.json`, with environment
    /// variables overriding file values.
    pub fn from_config() -> Result<Self, ClientError> {
        Self::from_config_path(default_config_path())
    }

    /// Create a client from a specific config path, with environment variables
    /// overriding file values.
    pub fn from_config_path(path: impl Into<PathBuf>) -> Result<Self, ClientError> {
        let path = path.into();
        let mut builder = Self::builder();
        if let Some(config) = read_client_config(&path)? {
            if !config.api_url.is_empty() {
                builder = builder.base_url(config.api_url);
            }
            if !config.api_key.is_empty() {
                builder = builder.api_key(config.api_key);
            }
        }
        builder.apply_env().build()
    }

    /// Access the generated OpenAPI client.
    pub fn api(&self) -> &crate::api::Client {
        &self.inner
    }

    /// Consume this wrapper and return the generated OpenAPI client.
    pub fn into_api(self) -> crate::api::Client {
        self.inner
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }
}

impl AsRef<crate::api::Client> for Client {
    fn as_ref(&self) -> &crate::api::Client {
        self.api()
    }
}

/// Builder for [`Client`].
#[derive(Clone, Debug)]
pub struct ClientBuilder {
    base_url: String,
    timeout: Duration,
    bearer_token: Option<String>,
    header_api_key: Option<String>,
    query_api_key: Option<String>,
    http_client: Option<reqwest::Client>,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            timeout: DEFAULT_TIMEOUT,
            bearer_token: None,
            header_api_key: None,
            query_api_key: None,
            http_client: None,
        }
    }
}

impl ClientBuilder {
    /// Override the default base URL.
    pub fn base_url(mut self, value: impl Into<String>) -> Self {
        self.base_url = value.into();
        self
    }

    /// Override the HTTP request timeout used by the default reqwest client.
    pub fn timeout(mut self, value: Duration) -> Self {
        self.timeout = value;
        self
    }

    /// Use a caller-provided reqwest client.
    ///
    /// SDK authentication and query options are still applied to each request.
    /// Configure any additional default headers on the reqwest builder.
    pub fn http_client(mut self, value: reqwest::Client) -> Self {
        self.http_client = Some(value);
        self
    }

    /// Send `Authorization: Bearer ...` on every request.
    pub fn bearer_token(mut self, value: impl Into<String>) -> Self {
        self.bearer_token = Some(value.into());
        self
    }

    /// Send the Nowledge Mem API key using both supported header forms.
    pub fn api_key(mut self, value: impl Into<String>) -> Self {
        let value = value.into();
        self.bearer_token = Some(value.clone());
        self.header_api_key = Some(value);
        self
    }

    /// Send `nmem_api_key=...` on every request.
    ///
    /// Prefer header authentication when possible. This exists for clients or
    /// proxies that strip custom headers.
    pub fn api_key_query(mut self, value: impl Into<String>) -> Self {
        self.query_api_key = Some(value.into());
        self
    }

    /// Apply `NMEM_API_URL` and `NMEM_API_KEY` from the environment.
    pub fn apply_env(mut self) -> Self {
        if let Ok(value) = std::env::var(ENV_API_URL)
            && !value.is_empty()
        {
            self = self.base_url(value);
        }
        if let Ok(value) = std::env::var(ENV_API_KEY)
            && !value.is_empty()
        {
            self = self.api_key(value);
        }
        self
    }

    /// Build the client.
    pub fn build(self) -> Result<Client, ClientError> {
        let base_url = parse_base_url(&self.base_url)?;
        let state = ClientState {
            bearer_token: self.bearer_token.map(bearer_header_value).transpose()?,
            header_api_key: self.header_api_key.map(api_key_header_value).transpose()?,
            query_api_key: self
                .query_api_key
                .map(|api_key| normalize_required_key(api_key, "query API key"))
                .transpose()?,
        };
        let http_client = match self.http_client {
            Some(client) => client,
            None => reqwest_client(self.timeout)?,
        };

        let generated_base_url = base_url.as_str().trim_end_matches('/').to_string();
        let inner = crate::api::Client::new_with_client(&generated_base_url, http_client, state);
        Ok(Client { inner, base_url })
    }
}

fn reqwest_client(timeout: Duration) -> Result<reqwest::Client, ClientError> {
    reqwest::ClientBuilder::new()
        .timeout(timeout)
        .build()
        .map_err(ClientError::BuildHttpClient)
}

fn bearer_header_value(token: impl AsRef<str>) -> Result<HeaderValue, ClientError> {
    let token = normalize_required_key(token, "bearer token")?;
    HeaderValue::from_str(&format!("Bearer {token}")).map_err(|source| {
        ClientError::InvalidHeaderValue {
            header: "Authorization",
            source,
        }
    })
}

fn api_key_header_value(api_key: impl AsRef<str>) -> Result<HeaderValue, ClientError> {
    let api_key = normalize_required_key(api_key, "API key")?;
    HeaderValue::from_str(&api_key).map_err(|source| ClientError::InvalidHeaderValue {
        header: HEADER_NMEM_API_KEY,
        source,
    })
}

fn parse_base_url(raw: &str) -> Result<Url, ClientError> {
    let url = Url::parse(raw).map_err(|source| ClientError::InvalidBaseUrl {
        value: raw.to_string(),
        source,
    })?;
    if url.query().is_some() {
        return Err(ClientError::BaseUrlCannotContainQuery {
            value: raw.to_string(),
        });
    }
    Ok(url)
}

fn normalize_required_key(
    value: impl AsRef<str>,
    kind: &'static str,
) -> Result<String, ClientError> {
    let value = normalize_bearer_token(value);
    if value.is_empty() {
        return Err(ClientError::EmptyApiKey { kind });
    }
    Ok(value)
}

fn normalize_bearer_token(token: impl AsRef<str>) -> String {
    let token = token.as_ref().trim();
    token
        .strip_prefix("Bearer ")
        .or_else(|| token.strip_prefix("bearer "))
        .unwrap_or(token)
        .trim()
        .to_string()
}

fn set_query_value(url: &mut Url, key: &str, value: &str) {
    let existing_pairs = url
        .query_pairs()
        .filter(|(existing_key, _)| existing_key != key)
        .map(|(existing_key, existing_value)| {
            (existing_key.into_owned(), existing_value.into_owned())
        })
        .collect::<Vec<_>>();
    url.set_query(None);
    let mut pairs = url.query_pairs_mut();
    for (existing_key, existing_value) in existing_pairs {
        pairs.append_pair(&existing_key, &existing_value);
    }
    pairs.append_pair(key, value);
}

fn default_config_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".nowledge-mem").join("config.json")
}

fn read_client_config(path: &Path) -> Result<Option<ClientConfig>, ClientError> {
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ClientError::ReadConfig {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    if data.is_empty() {
        return Ok(None);
    }
    serde_json::from_slice(&data)
        .map(Some)
        .map_err(|source| ClientError::DecodeConfig {
            path: path.to_path_buf(),
            source,
        })
}
