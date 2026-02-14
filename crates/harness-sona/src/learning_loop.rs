//! Background continuous learning loop.

use std::sync::Arc;

use harness_persistence::Repository;
use tokio::sync::watch;

use crate::error::SonaResult;
use crate::service::HiveLearningService;

/// Handle to a running background learning loop.
pub struct LoopHandle {
    /// Sender to signal the loop to stop.
    stop_tx: watch::Sender<bool>,
    /// Join handle for the background task.
    join_handle: tokio::task::JoinHandle<()>,
}

impl LoopHandle {
    /// Stop the background learning loop gracefully.
    pub async fn stop(self) -> SonaResult<()> {
        // Signal the loop to stop
        let _ = self.stop_tx.send(true);
        // Wait for it to finish
        let _ = self.join_handle.await;
        Ok(())
    }
}

/// Background learning loop that continuously aggregates contributions.
pub struct LearningLoop;

impl LearningLoop {
    /// Start the background learning loop.
    ///
    /// The loop runs aggregation rounds at the configured interval
    /// as long as there are pending contributions.
    pub fn start<R: Repository + 'static>(service: Arc<HiveLearningService<R>>) -> LoopHandle {
        let (stop_tx, mut stop_rx) = watch::channel(false);
        let interval = service.config().round_interval;

        let join_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {
                        // Check if we have pending contributions
                        if service.has_pending().await {
                            // Attempt aggregation, ignore errors (e.g. insufficient participants)
                            let _ = service.run_aggregation_round().await;
                        }
                    }
                    _ = stop_rx.changed() => {
                        // Final aggregation attempt before stopping
                        if service.has_pending().await {
                            let _ = service.run_aggregation_round().await;
                        }
                        break;
                    }
                }
            }
        });

        LoopHandle {
            stop_tx,
            join_handle,
        }
    }
}
