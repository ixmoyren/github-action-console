use std::sync::Arc;

use tokio::runtime::{Builder, Handle, Runtime};

/// The process-wide tokio runtime. Every GitHub call runs here; the UI only
/// awaits the resulting handle (ADR-0002).
#[derive(Clone)]
pub struct TokioRuntime {
    inner: Arc<Runtime>,
}

impl TokioRuntime {
    pub fn new() -> std::io::Result<Self> {
        let inner = Builder::new_multi_thread().enable_all().build()?;
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    pub fn handle(&self) -> Handle {
        self.inner.handle().clone()
    }

    /// Run a future to completion on the runtime. Composition-root use only:
    /// must not be called from inside the runtime.
    pub fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        self.inner.block_on(future)
    }

    pub fn spawn<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.inner.spawn(future)
    }
}
