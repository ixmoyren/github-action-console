use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::credentials;
use crate::github::{Account, DeviceFlowPoll, GatewayError, GitHubGateway, SecretToken};
use crate::store::Store;

const DEFAULT_HOST: &str = "github.com";

/// Why the user is looking at the login page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthProblem {
    DeviceFlowUnavailable,
    MissingClientId,
    Expired,
    Denied,
    InvalidCredentials,
    MissingScopes,
    Network,
    Unexpected,
}

impl AuthProblem {
    fn from_gateway(error: &GatewayError) -> Self {
        match error {
            GatewayError::DeviceFlowUnavailable => Self::DeviceFlowUnavailable,
            GatewayError::Unauthorized => Self::InvalidCredentials,
            GatewayError::Forbidden => Self::MissingScopes,
            GatewayError::Transport(_) => Self::Network,
            _ => Self::Unexpected,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthState {
    LoggedOut {
        notice: Option<AuthProblem>,
    },
    StartingDeviceFlow,
    AwaitingAuthorization {
        start: crate::github::DeviceFlowStart,
    },
    ValidatingCredentials,
    Authenticated {
        account: Account,
    },
}

/// Owns the login state machine. Depends only on the gateway seam, the local
/// store, and the OS keyring, so it is driveable without a UI or a network.
pub struct AuthManager {
    gateway: Arc<dyn GitHubGateway>,
    store: Store,
    state: AuthState,
    token: Option<SecretToken>,
    /// The OAuth App client id the pending device flow was started with.
    device_client_id: Option<String>,
    /// When the pending device code stops being valid.
    device_flow_deadline: Option<Instant>,
}

impl AuthManager {
    pub fn new(gateway: Arc<dyn GitHubGateway>, store: Store) -> Self {
        Self {
            gateway,
            store,
            state: AuthState::LoggedOut { notice: None },
            token: None,
            device_client_id: None,
            device_flow_deadline: None,
        }
    }

    pub fn state(&self) -> &AuthState {
        &self.state
    }

    /// Seconds left on the pending device code, counted down for the UI.
    pub fn remaining_secs(&self) -> Option<u64> {
        let deadline = self.device_flow_deadline?;
        Some(deadline.saturating_duration_since(Instant::now()).as_secs())
    }

    /// The active credential, if any. Needed by callers that read from GitHub
    /// on the user's behalf.
    pub fn token(&self) -> Option<SecretToken> {
        self.token.clone()
    }

    /// Try to resume a previous session from the keyring.
    pub async fn restore_session(&mut self) {
        let account = match self.store.load_account().await {
            Ok(Some(account)) => account,
            Ok(None) => {
                self.state = AuthState::LoggedOut { notice: None };
                return;
            }
            Err(_) => {
                self.state = AuthState::LoggedOut {
                    notice: Some(AuthProblem::Unexpected),
                };
                return;
            }
        };

        let token = match credentials::load_token(&account.login) {
            Ok(Some(token)) => token,
            Ok(None) => {
                self.state = AuthState::LoggedOut { notice: None };
                return;
            }
            Err(_) => {
                self.state = AuthState::LoggedOut {
                    notice: Some(AuthProblem::Unexpected),
                };
                return;
            }
        };

        match self.gateway.current_user(&token).await {
            Ok(account) => {
                let _ = self.store.save_account(&account, DEFAULT_HOST).await;
                self.token = Some(token);
                self.state = AuthState::Authenticated { account };
            }
            Err(error) => {
                if matches!(error, GatewayError::Unauthorized) {
                    let _ = credentials::delete_token(&account.login);
                }
                self.state = AuthState::LoggedOut {
                    notice: Some(AuthProblem::from_gateway(&error)),
                };
            }
        }
    }

    /// Begin a device flow with the client id the user typed.
    pub async fn start_device_flow(&mut self, client_id: &str) {
        let client_id = client_id.trim().to_owned();
        if client_id.is_empty() {
            self.state = AuthState::LoggedOut {
                notice: Some(AuthProblem::MissingClientId),
            };
            return;
        }

        self.state = AuthState::StartingDeviceFlow;
        match self.gateway.start_device_flow(&client_id).await {
            Ok(start) => {
                self.device_client_id = Some(client_id);
                self.device_flow_deadline =
                    Some(Instant::now() + Duration::from_secs(start.expires_in_secs));
                self.state = AuthState::AwaitingAuthorization { start };
            }
            Err(error) => {
                self.state = AuthState::LoggedOut {
                    notice: Some(AuthProblem::from_gateway(&error)),
                }
            }
        }
    }

    /// One poll of the pending authorization. Callers own the cadence.
    pub async fn poll_device_flow(&mut self) {
        let AuthState::AwaitingAuthorization { start } = &self.state else {
            return;
        };
        let handle = start.handle.clone();
        let Some(client_id) = self.device_client_id.clone() else {
            return;
        };

        match self.gateway.poll_device_flow(&client_id, &handle).await {
            Ok(DeviceFlowPoll::Pending) | Ok(DeviceFlowPoll::SlowDown) => {}
            Ok(DeviceFlowPoll::Expired) => {
                self.device_flow_deadline = None;
                self.state = AuthState::LoggedOut {
                    notice: Some(AuthProblem::Expired),
                }
            }
            Ok(DeviceFlowPoll::Denied) => {
                self.device_flow_deadline = None;
                self.state = AuthState::LoggedOut {
                    notice: Some(AuthProblem::Denied),
                }
            }
            Ok(DeviceFlowPoll::Authorized(token)) => {
                match self.gateway.current_user(&token).await {
                    Ok(account) => {
                        let _ = credentials::store_token(&account.login, &token);
                        let _ = self.store.save_account(&account, DEFAULT_HOST).await;
                        self.token = Some(token);
                        self.device_flow_deadline = None;
                        self.state = AuthState::Authenticated { account };
                    }
                    Err(error) => {
                        self.state = AuthState::LoggedOut {
                            notice: Some(AuthProblem::from_gateway(&error)),
                        }
                    }
                }
            }
            Err(error) => {
                self.state = AuthState::LoggedOut {
                    notice: Some(AuthProblem::from_gateway(&error)),
                }
            }
        }
    }

    /// Fallback sign-in with a pasted Personal Access Token.
    pub async fn sign_in_with_token(&mut self, raw_token: &str) {
        let token = SecretToken::new(raw_token.trim().to_owned());
        if token.expose().is_empty() {
            self.state = AuthState::LoggedOut {
                notice: Some(AuthProblem::InvalidCredentials),
            };
            return;
        }

        self.state = AuthState::ValidatingCredentials;
        match self.gateway.current_user(&token).await {
            Ok(account) => {
                let _ = credentials::store_token(&account.login, &token);
                let _ = self.store.save_account(&account, DEFAULT_HOST).await;
                self.token = Some(token);
                self.state = AuthState::Authenticated { account };
            }
            Err(error) => {
                self.state = AuthState::LoggedOut {
                    notice: Some(AuthProblem::from_gateway(&error)),
                }
            }
        }
    }

    pub async fn sign_out(&mut self) {
        if let Ok(Some(account)) = self.store.load_account().await {
            let _ = credentials::delete_token(&account.login);
        }
        let _ = self.store.clear_accounts().await;
        self.token = None;
        self.device_client_id = None;
        self.device_flow_deadline = None;
        self.state = AuthState::LoggedOut { notice: None };
    }
}
