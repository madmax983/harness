//! TUI interface for Harness v2 - Hive Mind dashboard.

mod app;
mod runner;
mod widgets;

pub use app::{AppMode, AppState, DashboardView};
pub use runner::TuiRunner;
