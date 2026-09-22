# One global tokio runtime, bridged to GPUI's own executors

GPUI ships its own smol-based `ForegroundExecutor`/`BackgroundExecutor` and does not bundle tokio, while octocrab is a tokio/reqwest stack. The app therefore owns a single process-wide `tokio::runtime::Runtime`; every octocrab call is issued on it, and results travel back to the UI through GPUI `Task`s or channels so UI state is only ever mutated from the GPUI side. Considered spawning octocrab inside `cx.background_executor()` (rejected: no tokio runtime context, easy to deadlock) and dropping octocrab for
hand-written reqwest calls (rejected: loses typed models for Actions and Releases).
