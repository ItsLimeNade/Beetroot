use anyhow::Result;
use tokio::sync::Semaphore;

/// How many chart renders / PNG encodes may run at once.
///
/// Each one holds tens of megabytes of pixel buffers, and the allocator keeps
/// the process at its high-water mark afterwards, so letting every concurrent
/// command render at the same time is what drives resident memory up. Extra
/// renders wait their turn; commands are already deferred by then.
const RENDER_CONCURRENCY: usize = 4;

static RENDER_SLOTS: Semaphore = Semaphore::const_new(RENDER_CONCURRENCY);

/// Run CPU- and memory-heavy image work on the blocking pool, at most
/// [`RENDER_CONCURRENCY`] at a time.
pub async fn run_blocking<T, F>(work: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    let _slot = RENDER_SLOTS.acquire().await?;
    tokio::task::spawn_blocking(work).await?
}
