use std::time::Duration;

use reqwest::header::{HeaderMap, RETRY_AFTER};

#[derive(Debug, Clone, Copy)]
pub(crate) struct RateLimitRetryPolicy {
    pub max_retries: u32,
    pub max_total_wait: Duration,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl RateLimitRetryPolicy {
    pub(crate) fn conservative() -> Self {
        Self {
            max_retries: 5,
            max_total_wait: Duration::from_secs(60),
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(16),
        }
    }
}

pub(crate) fn next_rate_limit_delay(
    headers: &HeaderMap,
    attempt: u32,
    total_waited: Duration,
    policy: RateLimitRetryPolicy,
) -> Option<Duration> {
    if attempt > policy.max_retries || total_waited >= policy.max_total_wait {
        return None;
    }

    let fallback = exponential_delay(policy.base_delay, attempt).min(policy.max_delay);
    let requested = retry_after_delay(headers).unwrap_or(fallback).min(policy.max_delay);
    let remaining = policy.max_total_wait.saturating_sub(total_waited);
    let delay = requested.min(remaining);
    if delay.is_zero() {
        None
    } else {
        Some(delay)
    }
}

fn exponential_delay(base: Duration, attempt: u32) -> Duration {
    let factor = 1u32.checked_shl(attempt.saturating_sub(1).min(10)).unwrap_or(1024);
    base.saturating_mul(factor)
}

fn retry_after_delay(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, RETRY_AFTER};

    #[test]
    fn test_retry_after_header_is_used_and_capped() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, "120".parse().unwrap());
        let policy = RateLimitRetryPolicy {
            max_retries: 3,
            max_total_wait: Duration::from_secs(30),
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(10),
        };

        let delay = next_rate_limit_delay(&headers, 1, Duration::ZERO, policy).unwrap();

        assert_eq!(delay, Duration::from_secs(10));
    }

    #[test]
    fn test_retry_stops_after_max_retries() {
        let headers = HeaderMap::new();
        let policy = RateLimitRetryPolicy {
            max_retries: 2,
            max_total_wait: Duration::from_secs(30),
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(10),
        };

        assert!(next_rate_limit_delay(&headers, 3, Duration::ZERO, policy).is_none());
    }

    #[test]
    fn test_retry_stops_when_total_wait_exhausted() {
        let headers = HeaderMap::new();
        let policy = RateLimitRetryPolicy {
            max_retries: 3,
            max_total_wait: Duration::from_secs(5),
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(10),
        };

        assert!(next_rate_limit_delay(&headers, 1, Duration::from_secs(5), policy).is_none());
    }
}
