use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use governor::{clock::DefaultClock, state::keyed::DefaultKeyedStateStore, Quota, RateLimiter};
use log::{debug, warn};
use std::{num::NonZeroU32, time::Duration};

use crate::admin::AdminAppState;

/// Rate limiter type for admin endpoints (keyed by admin user)
pub type AdminRateLimiter = RateLimiter<
    String,
    DefaultKeyedStateStore<String>,
    DefaultClock,
    governor::middleware::NoOpMiddleware,
>;

/// Rate limiting configuration for admin endpoints
#[derive(Debug, Clone)]
pub struct AdminRateLimitConfig {
    /// Maximum number of requests allowed per time window
    pub max_requests: NonZeroU32,
    /// Time window duration
    pub window: Duration,
}

impl Default for AdminRateLimitConfig {
    fn default() -> Self {
        Self {
            // Allow 5 login attempts per minute
            max_requests: NonZeroU32::new(5).expect("Rate limit max_requests must be non-zero"),
            window: Duration::from_secs(60),
        }
    }
}

impl AdminRateLimitConfig {
    /// Create a new rate limit configuration for login attempts
    pub fn for_login() -> Self {
        Self {
            // Allow only 5 login attempts per minute
            max_requests: NonZeroU32::new(5)
                .expect("Login rate limit max_requests must be non-zero"),
            window: Duration::from_secs(60),
        }
    }
}

/// Create a rate limiter with the given configuration
pub fn create_rate_limiter(config: AdminRateLimitConfig) -> AdminRateLimiter {
    let quota = Quota::with_period(config.window)
        .expect("Valid duration")
        .allow_burst(config.max_requests);

    RateLimiter::keyed(quota)
}

/// Admin rate limiter key - always "admin" since there's only one admin account
const ADMIN_KEY: &str = "admin";

/// Middleware specifically for login endpoints (5 attempts per minute)
pub async fn admin_login_rate_limit_middleware(
    State(state): State<AdminAppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    // Get the login rate limiter from the app state
    let rate_limiter = match state.admin_login_rate_limiter.as_ref() {
        Some(limiter) => limiter,
        None => {
            // No rate limiter configured, allow the request through
            debug!("No login rate limiter configured, allowing request");
            return next.run(request).await;
        }
    };

    let path = request.uri().path().to_string();

    match rate_limiter.check_key(&ADMIN_KEY.to_string()) {
        Ok(_) => {
            debug!("Login rate limit check passed for admin accessing {}", path);
            next.run(request).await
        }
        Err(_) => {
            warn!("Login rate limit exceeded for admin accessing {}", path);

            let mut response = Response::new(Body::from(
                "Too Many Login Attempts - Please try again in 1 minute",
            ));
            *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;

            // Retry after 60 seconds for login attempts
            if let Ok(retry_after) = "60".parse() {
                response.headers_mut().insert("retry-after", retry_after);
            }

            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AdminRateLimitConfig::default();
        assert_eq!(config.max_requests.get(), 5);
        assert_eq!(config.window, Duration::from_secs(60));
    }

    #[test]
    fn test_for_login_config() {
        let config = AdminRateLimitConfig::for_login();
        assert_eq!(config.max_requests.get(), 5);
        assert_eq!(config.window, Duration::from_secs(60));
    }

    #[test]
    fn test_create_rate_limiter() {
        let config = AdminRateLimitConfig::default();
        let _limiter = create_rate_limiter(config);
        // If we get here without panicking, the rate limiter was created successfully
    }

    #[test]
    fn test_login_rate_limiter_allows_5_attempts() {
        let config = AdminRateLimitConfig::for_login();
        let limiter = create_rate_limiter(config);

        // First 5 login attempts should succeed
        for _ in 0..5 {
            assert!(limiter.check_key(&ADMIN_KEY.to_string()).is_ok());
        }

        // 6th attempt should fail
        assert!(limiter.check_key(&ADMIN_KEY.to_string()).is_err());
    }

    #[test]
    fn test_rate_limiter_uses_admin_key() {
        let config = AdminRateLimitConfig::for_login();
        let limiter = create_rate_limiter(config);

        // Use up the limit with the admin key
        for _ in 0..5 {
            assert!(limiter.check_key(&ADMIN_KEY.to_string()).is_ok());
        }
        assert!(limiter.check_key(&ADMIN_KEY.to_string()).is_err());

        // Other keys should still work (different bucket)
        assert!(limiter.check_key(&"other_key".to_string()).is_ok());
    }
}
