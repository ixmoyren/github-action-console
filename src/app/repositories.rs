use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::github::{GatewayError, GitHubGateway, Repository, RepositorySort, SecretToken};
use crate::store::Store;

const REPOSITORY_PREFERENCE: &str = "workspace.repository";

/// A problem worth showing the user, independent of where it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppProblem {
    Forbidden,
    RateLimited,
    NotFound,
    Network,
    Unexpected,
}

impl AppProblem {
    pub(crate) fn from_gateway(error: &GatewayError) -> Self {
        match error {
            GatewayError::Forbidden => Self::Forbidden,
            GatewayError::RateLimited => Self::RateLimited,
            GatewayError::NotFound => Self::NotFound,
            GatewayError::Transport(_) => Self::Network,
            _ => Self::Unexpected,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryListState {
    Idle,
    Loading,
    Loaded,
    Failed(AppProblem),
}

const PAGE_SIZE: u32 = 30;

/// The repository picker. Owns paging, name filtering, sort order, and the
/// "last selected repository" preference.
pub struct RepositoryList {
    gateway: Arc<dyn GitHubGateway>,
    store: Store,
    state: RepositoryListState,
    repositories: Vec<Repository>,
    sort: RepositorySort,
    query: String,
    page: u32,
    has_more: bool,
    selected: Option<String>,
}

impl RepositoryList {
    pub fn new(gateway: Arc<dyn GitHubGateway>, store: Store) -> Self {
        Self {
            gateway,
            store,
            state: RepositoryListState::Idle,
            repositories: Vec::new(),
            sort: RepositorySort::Updated,
            query: String::new(),
            page: 0,
            has_more: false,
            selected: None,
        }
    }

    pub fn state(&self) -> RepositoryListState {
        self.state
    }

    pub fn sort(&self) -> RepositorySort {
        self.sort
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn has_more(&self) -> bool {
        self.has_more
    }

    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    /// Everything loaded so far, before filtering.
    pub fn repositories(&self) -> &[Repository] {
        &self.repositories
    }

    /// The loaded repositories matching the current query.
    pub fn visible(&self) -> Vec<Repository> {
        crate::github::filter_repositories(&self.repositories, &self.query)
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
    }

    pub fn set_sort(&mut self, sort: RepositorySort) {
        self.sort = sort;
    }

    /// Load the first page, replacing whatever is held.
    pub async fn reload(&mut self, token: &SecretToken) {
        self.state = RepositoryListState::Loading;
        match self
            .gateway
            .list_repositories(token, self.sort, 1, PAGE_SIZE)
            .await
        {
            Ok(page) => {
                info!(count = page.repositories.len(), "repositories loaded");
                self.repositories = page.repositories;
                self.has_more = page.has_more;
                self.page = 1;
                self.state = RepositoryListState::Loaded;
            }
            Err(error) => {
                self.state = RepositoryListState::Failed(AppProblem::from_gateway(&error))
            }
        }
    }

    /// Append the next page when one exists.
    pub async fn load_more(&mut self, token: &SecretToken) {
        if !self.has_more || self.state == RepositoryListState::Loading {
            return;
        }

        let next = self.page + 1;
        self.state = RepositoryListState::Loading;
        match self
            .gateway
            .list_repositories(token, self.sort, next, PAGE_SIZE)
            .await
        {
            Ok(page) => {
                debug!(
                    count = page.repositories.len(),
                    page = next,
                    "more repositories loaded"
                );
                self.repositories.extend(page.repositories);
                self.has_more = page.has_more;
                self.page = next;
                self.state = RepositoryListState::Loaded;
            }
            Err(error) => {
                self.state = RepositoryListState::Failed(AppProblem::from_gateway(&error))
            }
        }
    }

    /// Enter a repository and remember it for the next launch.
    pub async fn select(&mut self, full_name: &str) {
        self.selected = Some(full_name.to_owned());
        if let Err(error) = self
            .store
            .save_preference(REPOSITORY_PREFERENCE, full_name)
            .await
        {
            warn!(%error, "could not remember the selected repository");
        }
    }

    pub async fn leave_workspace(&mut self) {
        self.selected = None;
        if let Err(error) = self.store.clear_preference(REPOSITORY_PREFERENCE).await {
            warn!(%error, "could not clear the selected repository");
        }
    }

    /// Load the remembered repository, if any.
    pub async fn restore_selection(&mut self) {
        if self.selected.is_none()
            && let Ok(Some(full_name)) = self.store.load_preference(REPOSITORY_PREFERENCE).await
        {
            self.selected = Some(full_name);
        }
    }
}
