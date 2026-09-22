use std::collections::VecDeque;
use std::sync::{Arc, Mutex, Once};

use async_trait::async_trait;
use github_action_console::app::{AuthManager, AuthProblem, AuthState};
use github_action_console::credentials;
use github_action_console::github::{
    Account, DeviceFlowHandle, DeviceFlowPoll, DeviceFlowStart, GatewayError, GitHubGateway,
    SecretToken,
};
use github_action_console::store::Store;

fn keyring_mock() {
    static ONCE: Once = Once::new();
    ONCE.call_once(credentials::use_in_memory_backend);
}

#[derive(Default)]
struct FakeGateway {
    start: Mutex<VecDeque<Result<DeviceFlowStart, GatewayError>>>,
    polls: Mutex<VecDeque<Result<DeviceFlowPoll, GatewayError>>>,
    users: Mutex<VecDeque<Result<Account, GatewayError>>>,
}

impl FakeGateway {
    fn push_start(&self, response: Result<DeviceFlowStart, GatewayError>) {
        self.start.lock().unwrap().push_back(response);
    }

    fn push_poll(&self, response: Result<DeviceFlowPoll, GatewayError>) {
        self.polls.lock().unwrap().push_back(response);
    }

    fn push_user(&self, response: Result<Account, GatewayError>) {
        self.users.lock().unwrap().push_back(response);
    }
}

#[async_trait]
impl GitHubGateway for FakeGateway {
    async fn start_device_flow(&self) -> Result<DeviceFlowStart, GatewayError> {
        self.start
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected(
                "no scripted start".to_owned(),
            )))
    }

    async fn poll_device_flow(
        &self,
        _handle: &DeviceFlowHandle,
    ) -> Result<DeviceFlowPoll, GatewayError> {
        self.polls
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected("no scripted poll".to_owned())))
    }

    async fn current_user(&self, _token: &SecretToken) -> Result<Account, GatewayError> {
        self.users
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected("no scripted user".to_owned())))
    }

    async fn list_repositories(
        &self,
        _token: &SecretToken,
        _sort: github_action_console::github::RepositorySort,
        _page: u32,
        _per_page: u32,
    ) -> Result<github_action_console::github::RepositoryPage, GatewayError> {
        Err(GatewayError::Unexpected(
            "repository listing is not used in auth tests".to_owned(),
        ))
    }

    async fn list_workflows(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
    ) -> Result<Vec<github_action_console::github::Workflow>, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn list_workflow_runs(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _workflow_id: Option<u64>,
        _page: u32,
        _per_page: u32,
    ) -> Result<github_action_console::github::WorkflowRunPage, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn list_jobs(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _run_id: u64,
    ) -> Result<Vec<github_action_console::github::Job>, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn job_logs(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _job_id: u64,
    ) -> Result<String, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn run_logs_archive(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _run_id: u64,
    ) -> Result<Vec<u8>, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn list_artifacts(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _run_id: u64,
    ) -> Result<Vec<github_action_console::github::BuildArtifact>, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn download_artifact(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
        _artifact_id: u64,
    ) -> Result<Vec<u8>, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }
}

fn device_start() -> DeviceFlowStart {
    DeviceFlowStart {
        user_code: "ABCD-1234".to_owned(),
        verification_uri: "https://github.com/login/device".to_owned(),
        expires_in_secs: 900,
        interval_secs: 5,
        handle: DeviceFlowHandle::new("device-code"),
    }
}

fn account(login: &str) -> Account {
    Account {
        login: login.to_owned(),
    }
}

fn token() -> SecretToken {
    SecretToken::new("gho_test_token")
}

async fn manager(gateway: Arc<FakeGateway>) -> AuthManager {
    keyring_mock();
    let store = Store::in_memory().await.unwrap();
    AuthManager::new(gateway, store)
}

#[tokio::test]
async fn restore_session_without_stored_account_stays_logged_out() {
    let manager = manager(Arc::new(FakeGateway::default())).await;
    let mut manager = manager;

    manager.restore_session().await;

    assert_eq!(*manager.state(), AuthState::LoggedOut { notice: None });
}

#[tokio::test]
async fn restore_session_with_valid_token_authenticates() {
    keyring_mock();
    let store = Store::in_memory().await.unwrap();
    store
        .save_account(&account("carol"), "github.com")
        .await
        .unwrap();
    credentials::store_token("carol", &token()).unwrap();

    let gateway = Arc::new(FakeGateway::default());
    gateway.push_user(Ok(account("carol")));
    let mut manager = AuthManager::new(gateway, store);

    manager.restore_session().await;

    assert_eq!(
        *manager.state(),
        AuthState::Authenticated {
            account: account("carol")
        }
    );
}

#[tokio::test]
async fn restore_session_with_revoked_token_clears_credentials() {
    keyring_mock();
    let store = Store::in_memory().await.unwrap();
    store
        .save_account(&account("dave"), "github.com")
        .await
        .unwrap();
    credentials::store_token("dave", &token()).unwrap();

    let gateway = Arc::new(FakeGateway::default());
    gateway.push_user(Err(GatewayError::Unauthorized));
    let mut manager = AuthManager::new(gateway, store);

    manager.restore_session().await;

    assert_eq!(
        *manager.state(),
        AuthState::LoggedOut {
            notice: Some(AuthProblem::InvalidCredentials)
        }
    );
    assert_eq!(credentials::load_token("dave").unwrap(), None);
}

#[tokio::test]
async fn device_flow_pending_then_authorized_persists_token_and_account() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_start(Ok(device_start()));
    gateway.push_poll(Ok(DeviceFlowPoll::Pending));
    gateway.push_poll(Ok(DeviceFlowPoll::Authorized(token())));
    gateway.push_user(Ok(account("erin")));
    let mut manager = manager(gateway).await;

    manager.start_device_flow().await;
    assert!(matches!(
        manager.state(),
        AuthState::AwaitingAuthorization { .. }
    ));

    manager.poll_device_flow().await;
    assert!(matches!(
        manager.state(),
        AuthState::AwaitingAuthorization { .. }
    ));

    manager.poll_device_flow().await;
    assert_eq!(
        *manager.state(),
        AuthState::Authenticated {
            account: account("erin")
        }
    );
    assert_eq!(credentials::load_token("erin").unwrap(), Some(token()));
}

#[tokio::test]
async fn expired_device_flow_returns_a_notice() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_start(Ok(device_start()));
    gateway.push_poll(Ok(DeviceFlowPoll::Expired));
    let mut manager = manager(gateway).await;

    manager.start_device_flow().await;
    manager.poll_device_flow().await;

    assert_eq!(
        *manager.state(),
        AuthState::LoggedOut {
            notice: Some(AuthProblem::Expired)
        }
    );
}

#[tokio::test]
async fn denied_device_flow_returns_a_notice() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_start(Ok(device_start()));
    gateway.push_poll(Ok(DeviceFlowPoll::Denied));
    let mut manager = manager(gateway).await;

    manager.start_device_flow().await;
    manager.poll_device_flow().await;

    assert_eq!(
        *manager.state(),
        AuthState::LoggedOut {
            notice: Some(AuthProblem::Denied)
        }
    );
}

#[tokio::test]
async fn device_flow_unavailable_returns_a_notice() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_start(Err(GatewayError::DeviceFlowUnavailable));
    let mut manager = manager(gateway).await;

    manager.start_device_flow().await;

    assert_eq!(
        *manager.state(),
        AuthState::LoggedOut {
            notice: Some(AuthProblem::DeviceFlowUnavailable)
        }
    );
}

#[tokio::test]
async fn sign_out_clears_token_and_account() {
    keyring_mock();
    let store = Store::in_memory().await.unwrap();
    store
        .save_account(&account("frank"), "github.com")
        .await
        .unwrap();
    credentials::store_token("frank", &token()).unwrap();

    let gateway = Arc::new(FakeGateway::default());
    gateway.push_user(Ok(account("frank")));
    let mut manager = AuthManager::new(gateway, store);
    manager.restore_session().await;
    assert!(matches!(manager.state(), AuthState::Authenticated { .. }));

    manager.sign_out().await;

    assert_eq!(*manager.state(), AuthState::LoggedOut { notice: None });
    assert_eq!(credentials::load_token("frank").unwrap(), None);
}

#[tokio::test]
async fn pat_login_with_valid_token_authenticates() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_user(Ok(account("grace")));
    let mut manager = manager(gateway).await;

    manager.sign_in_with_token("  ghp_pasted_token  ").await;

    assert_eq!(
        *manager.state(),
        AuthState::Authenticated {
            account: account("grace")
        }
    );
    assert_eq!(
        credentials::load_token("grace").unwrap(),
        Some(SecretToken::new("ghp_pasted_token"))
    );
    assert_eq!(
        manager.token().unwrap(),
        SecretToken::new("ghp_pasted_token")
    );
}

#[tokio::test]
async fn pat_login_without_scope_reports_missing_scopes() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_user(Err(GatewayError::Forbidden));
    let mut manager = manager(gateway).await;

    manager.sign_in_with_token("ghp_pasted_token").await;

    assert_eq!(
        *manager.state(),
        AuthState::LoggedOut {
            notice: Some(AuthProblem::MissingScopes)
        }
    );
}

#[tokio::test]
async fn pat_login_with_blank_token_reports_invalid_credentials() {
    let gateway = Arc::new(FakeGateway::default());
    let mut manager = manager(gateway).await;

    manager.sign_in_with_token("   ").await;

    assert_eq!(
        *manager.state(),
        AuthState::LoggedOut {
            notice: Some(AuthProblem::InvalidCredentials)
        }
    );
}

#[tokio::test]
async fn pat_login_after_device_flow_unavailable_succeeds() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_start(Err(GatewayError::DeviceFlowUnavailable));
    gateway.push_user(Ok(account("heidi")));
    let mut manager = manager(gateway).await;

    manager.start_device_flow().await;
    assert_eq!(
        *manager.state(),
        AuthState::LoggedOut {
            notice: Some(AuthProblem::DeviceFlowUnavailable)
        }
    );

    manager.sign_in_with_token("ghp_pasted_token").await;

    assert_eq!(
        *manager.state(),
        AuthState::Authenticated {
            account: account("heidi")
        }
    );
}
