pub mod octocrab_client;

use async_trait::async_trait;
use thiserror::Error;

/// The identity the console is currently acting as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub login: String,
}

/// A GitHub access token. Never rendered or logged.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretToken(String);

impl SecretToken {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretToken(***)")
    }
}

/// Opaque handle to an in-flight device-flow authorization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFlowHandle(String);

impl DeviceFlowHandle {
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    pub fn code(&self) -> &str {
        &self.0
    }
}

/// What the user needs in order to authorize this device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFlowStart {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in_secs: u64,
    pub interval_secs: u64,
    pub handle: DeviceFlowHandle,
}

/// The result of one poll against GitHub's device-flow token endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceFlowPoll {
    Pending,
    SlowDown,
    Authorized(SecretToken),
    Expired,
    Denied,
}

/// How repository listings are ordered by GitHub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositorySort {
    Updated,
    Pushed,
}

impl RepositorySort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Updated => "updated",
            Self::Pushed => "pushed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repository {
    pub name: String,
    pub full_name: String,
    pub is_private: bool,
}

impl Repository {
    /// Case-insensitive match against the repository name or full name.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        self.name.to_lowercase().contains(&query) || self.full_name.to_lowercase().contains(&query)
    }
}

/// The single name filter shared by the picker and its view.
pub fn filter_repositories(repositories: &[Repository], query: &str) -> Vec<Repository> {
    repositories
        .iter()
        .filter(|repository| repository.matches(query))
        .cloned()
        .collect()
}

/// One page of repositories plus whether another page exists.#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RepositoryPage {
    pub repositories: Vec<Repository>,
    pub has_more: bool,
}

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("device flow is not configured")]
    DeviceFlowUnavailable,
    #[error("credentials are no longer valid")]
    Unauthorized,
    #[error("insufficient permissions")]
    Forbidden,
    #[error("rate limited")]
    RateLimited,
    #[error("not found")]
    NotFound,
    #[error("transport failure: {0}")]
    Transport(String),
    #[error("unexpected response: {0}")]
    Unexpected(String),
}

/// The single seam between the application and GitHub.
#[async_trait]
pub trait GitHubGateway: Send + Sync {
    async fn start_device_flow(&self) -> Result<DeviceFlowStart, GatewayError>;

    async fn poll_device_flow(
        &self,
        handle: &DeviceFlowHandle,
    ) -> Result<DeviceFlowPoll, GatewayError>;

    async fn current_user(&self, token: &SecretToken) -> Result<Account, GatewayError>;

    async fn list_repositories(
        &self,
        token: &SecretToken,
        sort: RepositorySort,
        page: u32,
        per_page: u32,
    ) -> Result<RepositoryPage, GatewayError>;
}
