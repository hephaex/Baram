//! Retry utilities for resilient operations
//!
//! This module provides a common retry mechanism with exponential backoff
//! that can be used across different parts of the application (parser, indexer, etc.).

use anyhow::Result;
use std::future::Future;
use std::time::Duration;
use tracing::{debug, warn};

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,

    /// Base delay in milliseconds for exponential backoff
    pub base_delay_ms: u64,

    /// Maximum delay in milliseconds (caps exponential growth)
    pub max_delay_ms: u64,

    /// Multiplier for exponential backoff (default: 2.0)
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 30_000,
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// Create a new retry configuration with custom max retries
    pub fn new(max_retries: u32) -> Self {
        Self {
            max_retries,
            ..Default::default()
        }
    }

    /// Create a retry configuration with custom delays
    pub fn with_delays(max_retries: u32, base_delay_ms: u64, max_delay_ms: u64) -> Self {
        Self {
            max_retries,
            base_delay_ms,
            max_delay_ms,
            backoff_multiplier: 2.0,
        }
    }

    /// Calculate delay for a given attempt using exponential backoff
    fn calculate_delay(&self, attempt: u32) -> Duration {
        let delay_ms = if attempt == 0 {
            0
        } else {
            let exponential =
                self.base_delay_ms as f64 * self.backoff_multiplier.powi((attempt - 1) as i32);
            (exponential as u64).min(self.max_delay_ms)
        };

        Duration::from_millis(delay_ms)
    }
}

/// Execute an operation with retry logic and exponential backoff
///
/// # Arguments
///
/// * `config` - Retry configuration
/// * `operation` - Async operation to retry (must return Result<T>)
///
/// # Returns
///
/// Returns `Ok(T)` on success, or the last error if all retries fail
///
/// # Example
///
/// ```no_run
/// use baram::utils::retry::{with_retry, RetryConfig};
/// use anyhow::Result;
///
/// async fn fetch_data() -> Result<String> {
///     // Your operation here
///     Ok("data".to_string())
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<()> {
///     let config = RetryConfig::default();
///     let result = with_retry(&config, || async {
///         fetch_data().await
///     }).await?;
///     Ok(())
/// }
/// ```
pub async fn with_retry<T, F, Fut>(config: &RetryConfig, operation: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        // Apply exponential backoff for retries
        if attempt > 0 {
            let delay = config.calculate_delay(attempt);
            debug!(
                attempt = attempt,
                delay_ms = delay.as_millis(),
                "Retrying operation after delay"
            );
            tokio::time::sleep(delay).await;
        }

        // Execute operation
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!(attempt = attempt, "Operation succeeded after retry");
                }
                return Ok(result);
            }
            Err(e) => {
                warn!(
                    attempt = attempt,
                    max_retries = config.max_retries,
                    error = %e,
                    "Operation failed"
                );
                last_error = Some(e);
            }
        }
    }

    // All retries exhausted
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Operation failed with no error details")))
}

/// Execute an operation with retry logic, using a custom retry predicate
///
/// This variant allows you to specify which errors should trigger a retry.
///
/// # Arguments
///
/// * `config` - Retry configuration
/// * `operation` - Async operation to retry
/// * `should_retry` - Predicate function that determines if an error should trigger retry
///
/// # Returns
///
/// Returns `Ok(T)` on success, or the last error if all retries fail or retry is not warranted
///
/// # Example
///
/// ```no_run
/// use baram::utils::retry::{with_retry_if, RetryConfig};
/// use anyhow::Result;
///
/// async fn fetch_data() -> Result<String> {
///     // Your operation here
///     Ok("data".to_string())
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<()> {
///     let config = RetryConfig::default();
///     let result = with_retry_if(
///         &config,
///         || async { fetch_data().await },
///         |e| {
///             // Only retry on network errors, not on validation errors
///             e.to_string().contains("network")
///         }
///     ).await?;
///     Ok(())
/// }
/// ```
pub async fn with_retry_if<T, F, Fut, P>(
    config: &RetryConfig,
    operation: F,
    should_retry: P,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
    P: Fn(&anyhow::Error) -> bool,
{
    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        // Apply exponential backoff for retries
        if attempt > 0 {
            let delay = config.calculate_delay(attempt);
            debug!(
                attempt = attempt,
                delay_ms = delay.as_millis(),
                "Retrying operation after delay"
            );
            tokio::time::sleep(delay).await;
        }

        // Execute operation
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!(attempt = attempt, "Operation succeeded after retry");
                }
                return Ok(result);
            }
            Err(e) => {
                // Check if we should retry this error
                if !should_retry(&e) {
                    warn!(error = %e, "Non-retryable error encountered");
                    return Err(e);
                }

                warn!(
                    attempt = attempt,
                    max_retries = config.max_retries,
                    error = %e,
                    "Operation failed, will retry"
                );
                last_error = Some(e);
            }
        }
    }

    // All retries exhausted
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Operation failed with no error details")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_success_first_attempt() {
        let config = RetryConfig::new(3);
        let result = with_retry(&config, || async { Ok::<_, anyhow::Error>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let config = RetryConfig::new(3);
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = Arc::clone(&attempts);

        let result = with_retry(&config, move || {
            let attempts = Arc::clone(&attempts_clone);
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    anyhow::bail!("Simulated failure");
                }
                Ok::<_, anyhow::Error>(42)
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let config = RetryConfig::new(2);
        let result: Result<(), anyhow::Error> =
            with_retry(&config, || async { anyhow::bail!("Permanent failure") }).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Permanent failure"));
    }

    #[tokio::test]
    async fn test_retry_if_predicate() {
        let config = RetryConfig::new(3);

        // Should not retry validation errors
        let result: Result<(), anyhow::Error> = with_retry_if(
            &config,
            || async { anyhow::bail!("validation error") },
            |e| !e.to_string().contains("validation"),
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("validation"));
    }

    #[test]
    fn test_calculate_delay() {
        let config = RetryConfig::default();

        assert_eq!(config.calculate_delay(0), Duration::from_millis(0));
        assert_eq!(config.calculate_delay(1), Duration::from_millis(1000));
        assert_eq!(config.calculate_delay(2), Duration::from_millis(2000));
        assert_eq!(config.calculate_delay(3), Duration::from_millis(4000));
    }

    #[test]
    fn test_max_delay_cap() {
        let config = RetryConfig::with_delays(10, 1000, 5000);

        // Should not exceed max_delay_ms
        assert_eq!(config.calculate_delay(10), Duration::from_millis(5000));
    }
}
