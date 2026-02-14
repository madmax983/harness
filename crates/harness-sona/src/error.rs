//! Error types for SONA operations.

use thiserror::Error;

/// Errors from SONA collective learning operations.
#[derive(Debug, Error)]
pub enum SonaError {
    #[error("stale delta: timestamp is older than staleness threshold")]
    StaleDelta,

    #[error("insufficient participants: need {required}, got {actual}")]
    InsufficientParticipants { required: usize, actual: usize },

    #[error("unknown round: {0}")]
    UnknownRound(String),

    #[error("round already complete: {0}")]
    RoundAlreadyComplete(String),

    #[error("no contributions to aggregate")]
    NoContributions,

    #[error("domain not found: {0}")]
    DomainNotFound(String),

    #[error("not initialized")]
    NotInitialized,

    #[error("persistence error: {0}")]
    Persistence(String),

    #[error("ewc error: {0}")]
    Ewc(#[from] crate::ewc::EwcError),

    #[error("engine not configured: {0}")]
    NotConfigured(String),
}

/// Type alias for SONA results.
pub type SonaResult<T> = Result<T, SonaError>;
