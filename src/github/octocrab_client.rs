use octocrab::Octocrab;
use octocrab::auth::DeviceCodes;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use super::{
    Account, DeviceFlowHandle, DeviceFlowPoll, DeviceFlowStart, GatewayError, GitHubGateway,
    Repository, RepositoryPage, RepositorySort, SecretToken,
};

const DEFAULT_BASE_URI: &str = "https://github.com";
const DEVICE_FLOW_SCOPES: [&str; 2] = ["repo", "workflow"];
const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// octocrab-backed gateway. Thin mapping only: no retry policy, no caching.
pub struct OctocrabGateway {
    client_id: Option<String>,
    base_uri: String,
}

impl OctocrabGateway {
    pub fn new(client_id: Option<String>) -> Self {
        Self {
            client_id,
            base_uri: DEFAULT_BASE_URI.to_owned(),
        }
    }

    fn client_id(&self) -> Result<&str, GatewayError> {
        self.client_id
            .as_deref()
            .ok_or(GatewayError::DeviceFlowUnavailable)
    }

    /// Device flow must talk to the web host and ask for JSON.
    fn device_flow_client(&self) -> Result<Octocrab, GatewayError> {
        Octocrab::builder()
            .base_uri(&self.base_uri)
            .map_err(unexpected)?
            .add_header(http::header::ACCEPT, "application/json".to_owned())
            .build()
            .map_err(unexpected)
    }
}

#[async_trait::async_trait]
impl GitHubGateway for OctocrabGateway {
    async fn start_device_flow(&self) -> Result<DeviceFlowStart, GatewayError> {
        let client_id = self.client_id()?;
        let crab = self.device_flow_client()?;
        let codes: DeviceCodes = crab
            .authenticate_as_device(
                &SecretString::from(client_id.to_owned()),
                DEVICE_FLOW_SCOPES,
            )
            .await
            .map_err(map_error)?;

        Ok(DeviceFlowStart {
            user_code: codes.user_code,
            verification_uri: codes.verification_uri,
            expires_in_secs: codes.expires_in,
            interval_secs: codes.interval,
            handle: DeviceFlowHandle::new(codes.device_code),
        })
    }

    async fn poll_device_flow(
        &self,
        handle: &DeviceFlowHandle,
    ) -> Result<DeviceFlowPoll, GatewayError> {
        let client_id = self.client_id()?;
        let crab = self.device_flow_client()?;
        let body = PollBody {
            client_id,
            device_code: handle.code(),
            grant_type: DEVICE_GRANT_TYPE,
        };

        // GitHub answers 200 with either `access_token` or `error`, so the
        // response is decoded here rather than through octocrab's typed
        // `TokenResponse`, which cannot represent `expired_token`/`access_denied`.
        let response: PollResponse = crab
            .post("/login/oauth/access_token", Some(&body))
            .await
            .map_err(map_error)?;

        if let Some(token) = response.access_token {
            return Ok(DeviceFlowPoll::Authorized(SecretToken::new(token)));
        }

        match response.error.as_deref() {
            Some("authorization_pending") => Ok(DeviceFlowPoll::Pending),
            Some("slow_down") => Ok(DeviceFlowPoll::SlowDown),
            Some("expired_token") => Ok(DeviceFlowPoll::Expired),
            Some("access_denied") => Ok(DeviceFlowPoll::Denied),
            Some(other) => Err(GatewayError::Unexpected(other.to_owned())),
            None => Err(GatewayError::Unexpected(
                "empty device flow response".to_owned(),
            )),
        }
    }

    async fn current_user(&self, token: &SecretToken) -> Result<Account, GatewayError> {
        let crab = user_client(token)?;
        let author = crab.current().user().await.map_err(map_error)?;
        Ok(Account {
            login: author.login,
        })
    }

    async fn list_repositories(
        &self,
        token: &SecretToken,
        sort: RepositorySort,
        page: u32,
        per_page: u32,
    ) -> Result<RepositoryPage, GatewayError> {
        let crab = user_client(token)?;
        let page = crab
            .current()
            .list_repos_for_authenticated_user()
            .visibility("all")
            .affiliation("owner,collaborator,organization_member")
            .type_("all")
            .sort(sort.as_str())
            .direction("desc")
            .per_page(per_page.clamp(1, 100) as u8)
            .page(page.clamp(1, 255) as u8)
            .send()
            .await
            .map_err(map_error)?;

        Ok(RepositoryPage {
            repositories: page
                .items
                .into_iter()
                .map(|repository| Repository {
                    name: repository.name,
                    full_name: repository.full_name.unwrap_or_default(),
                    is_private: repository.private.unwrap_or(false),
                })
                .collect(),
            has_more: page.next.is_some(),
        })
    }
}

fn user_client(token: &SecretToken) -> Result<Octocrab, GatewayError> {
    Octocrab::builder()
        .user_access_token(token.expose().to_owned())
        .build()
        .map_err(unexpected)
}

#[derive(Serialize)]
struct PollBody<'a> {
    client_id: &'a str,
    device_code: &'a str,
    grant_type: &'a str,
}

#[derive(Deserialize)]
struct PollResponse {
    access_token: Option<String>,
    error: Option<String>,
}

fn unexpected(error: impl std::fmt::Display) -> GatewayError {
    GatewayError::Unexpected(error.to_string())
}

fn map_error(error: octocrab::Error) -> GatewayError {
    match &error {
        octocrab::Error::GitHub { source, .. } => match source.status_code.as_u16() {
            401 => GatewayError::Unauthorized,
            403 => GatewayError::RateLimited,
            404 => GatewayError::NotFound,
            _ => GatewayError::Unexpected(source.message.clone()),
        },
        octocrab::Error::Http { .. } | octocrab::Error::Service { .. } => {
            GatewayError::Transport(error.to_string())
        }
        _ => GatewayError::Transport(error.to_string()),
    }
}
