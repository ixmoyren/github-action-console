use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use github_action_console::app::{AppProblem, RepositoryList, RepositoryListState};
use github_action_console::github::{
    Account, DeviceFlowHandle, DeviceFlowPoll, DeviceFlowStart, GatewayError, GitHubGateway,
    Repository, RepositoryPage, RepositorySort, SecretToken, Workflow, WorkflowRunPage,
};
use github_action_console::store::Store;

#[derive(Default)]
struct FakeGateway {
    pages: Mutex<VecDeque<Result<RepositoryPage, GatewayError>>>,
    requests: Mutex<Vec<(RepositorySort, u32)>>,
}

impl FakeGateway {
    fn push_page(&self, response: Result<RepositoryPage, GatewayError>) {
        self.pages.lock().unwrap().push_back(response);
    }

    fn requests(&self) -> Vec<(RepositorySort, u32)> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait]
impl GitHubGateway for FakeGateway {
    async fn start_device_flow(&self) -> Result<DeviceFlowStart, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn poll_device_flow(
        &self,
        _handle: &DeviceFlowHandle,
    ) -> Result<DeviceFlowPoll, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn current_user(&self, _token: &SecretToken) -> Result<Account, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }

    async fn list_repositories(
        &self,
        _token: &SecretToken,
        sort: RepositorySort,
        page: u32,
        _per_page: u32,
    ) -> Result<RepositoryPage, GatewayError> {
        self.requests.lock().unwrap().push((sort, page));
        self.pages
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(GatewayError::Unexpected("no scripted page".to_owned())))
    }

    async fn list_workflows(
        &self,
        _token: &SecretToken,
        _owner: &str,
        _repository: &str,
    ) -> Result<Vec<Workflow>, GatewayError> {
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
    ) -> Result<WorkflowRunPage, GatewayError> {
        Err(GatewayError::Unexpected("unused".to_owned()))
    }
}

fn repository(full_name: &str, is_private: bool) -> Repository {
    Repository {
        name: full_name.rsplit('/').next().unwrap_or(full_name).to_owned(),
        full_name: full_name.to_owned(),
        is_private,
    }
}

fn page(repositories: Vec<Repository>, has_more: bool) -> RepositoryPage {
    RepositoryPage {
        repositories,
        has_more,
    }
}

fn token() -> SecretToken {
    SecretToken::new("ghp_test_token")
}

async fn list_with(gateway: Arc<FakeGateway>) -> RepositoryList {
    let store = Store::in_memory().await.unwrap();
    RepositoryList::new(gateway, store)
}

#[tokio::test]
async fn reload_loads_the_first_page() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_page(Ok(page(
        vec![
            repository("octo/alpha", false),
            repository("octo/beta", true),
        ],
        true,
    )));
    let mut list = list_with(gateway.clone()).await;

    list.reload(&token()).await;

    assert_eq!(list.state(), RepositoryListState::Loaded);
    assert_eq!(list.repositories().len(), 2);
    assert!(list.has_more());
    assert_eq!(gateway.requests(), vec![(RepositorySort::Updated, 1)]);
}

#[tokio::test]
async fn private_flag_is_preserved() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_page(Ok(page(
        vec![
            repository("octo/alpha", false),
            repository("octo/beta", true),
        ],
        false,
    )));
    let mut list = list_with(gateway).await;

    list.reload(&token()).await;

    assert!(!list.repositories()[0].is_private);
    assert!(list.repositories()[1].is_private);
}

#[tokio::test]
async fn load_more_appends_the_next_page() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_page(Ok(page(vec![repository("octo/alpha", false)], true)));
    gateway.push_page(Ok(page(vec![repository("octo/beta", false)], false)));
    let mut list = list_with(gateway.clone()).await;

    list.reload(&token()).await;
    list.load_more(&token()).await;

    assert_eq!(list.repositories().len(), 2);
    assert!(!list.has_more());
    assert_eq!(
        gateway.requests(),
        vec![(RepositorySort::Updated, 1), (RepositorySort::Updated, 2)]
    );
}

#[tokio::test]
async fn load_more_is_a_no_op_when_nothing_follows() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_page(Ok(page(vec![repository("octo/alpha", false)], false)));
    let mut list = list_with(gateway.clone()).await;

    list.reload(&token()).await;
    list.load_more(&token()).await;

    assert_eq!(list.state(), RepositoryListState::Loaded);
    assert_eq!(gateway.requests().len(), 1);
}

#[tokio::test]
async fn search_filters_by_name_case_insensitively() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_page(Ok(page(
        vec![
            repository("octo/alpha", false),
            repository("octo/beta", false),
            repository("other/gamma", false),
        ],
        false,
    )));
    let mut list = list_with(gateway).await;

    list.reload(&token()).await;
    list.set_query("BET");

    let visible = list.visible();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].full_name, "octo/beta");
}

#[tokio::test]
async fn empty_query_shows_everything() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_page(Ok(page(
        vec![
            repository("octo/alpha", false),
            repository("octo/beta", false),
        ],
        false,
    )));
    let mut list = list_with(gateway).await;

    list.reload(&token()).await;
    list.set_query("   ");

    assert_eq!(list.visible().len(), 2);
}

#[tokio::test]
async fn changing_sort_refetches_with_the_new_order() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_page(Ok(page(vec![repository("octo/alpha", false)], false)));
    gateway.push_page(Ok(page(vec![repository("octo/beta", false)], false)));
    let mut list = list_with(gateway.clone()).await;

    list.reload(&token()).await;
    list.set_sort(RepositorySort::Pushed);
    list.reload(&token()).await;

    assert_eq!(list.sort(), RepositorySort::Pushed);
    assert_eq!(
        gateway.requests(),
        vec![(RepositorySort::Updated, 1), (RepositorySort::Pushed, 1)]
    );
}

#[tokio::test]
async fn an_empty_page_is_a_loaded_empty_state() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_page(Ok(page(Vec::new(), false)));
    let mut list = list_with(gateway).await;

    list.reload(&token()).await;

    assert_eq!(list.state(), RepositoryListState::Loaded);
    assert!(list.visible().is_empty());
}

#[tokio::test]
async fn transport_failure_is_surfaced() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_page(Err(GatewayError::Transport("offline".to_owned())));
    let mut list = list_with(gateway).await;

    list.reload(&token()).await;

    assert_eq!(
        list.state(),
        RepositoryListState::Failed(AppProblem::Network)
    );
}

#[tokio::test]
async fn selection_is_remembered_across_instances() {
    let gateway = Arc::new(FakeGateway::default());
    gateway.push_page(Ok(page(vec![repository("octo/alpha", false)], false)));
    let store = Store::in_memory().await.unwrap();

    let mut first = RepositoryList::new(gateway.clone(), store.clone());
    first.reload(&token()).await;
    first.select("octo/alpha").await;
    assert_eq!(first.selected(), Some("octo/alpha"));

    let mut second = RepositoryList::new(gateway, store);
    second.restore_selection().await;

    assert_eq!(second.selected(), Some("octo/alpha"));

    second.leave_workspace().await;
    assert_eq!(second.selected(), None);
}
