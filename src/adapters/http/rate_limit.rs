use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use uuid::Uuid;

const WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Default)]
pub(crate) struct ApiKeyMinuteLimiter {
    windows: Arc<Mutex<HashMap<Uuid, MinuteWindow>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MinuteReservation {
    api_key_id: Uuid,
    window_started_at: Instant,
}

#[derive(Clone, Copy, Debug)]
struct MinuteWindow {
    started_at: Instant,
    used: u32,
}

impl ApiKeyMinuteLimiter {
    pub(crate) fn reserve(
        &self,
        api_key_id: Uuid,
        requests_per_minute: i32,
    ) -> Option<MinuteReservation> {
        self.reserve_at(api_key_id, requests_per_minute, Instant::now())
    }

    pub(crate) fn release(&self, reservation: MinuteReservation) {
        let mut windows = self.windows.lock().expect("minute limiter mutex poisoned");
        let Some(window) = windows.get_mut(&reservation.api_key_id) else {
            return;
        };

        if window.started_at == reservation.window_started_at && window.used > 0 {
            window.used -= 1;
        }
    }

    fn reserve_at(
        &self,
        api_key_id: Uuid,
        requests_per_minute: i32,
        now: Instant,
    ) -> Option<MinuteReservation> {
        let limit = u32::try_from(requests_per_minute).ok()?;
        if limit == 0 {
            return None;
        }

        let mut windows = self.windows.lock().expect("minute limiter mutex poisoned");
        let window = windows.entry(api_key_id).or_insert(MinuteWindow {
            started_at: now,
            used: 0,
        });

        if now.duration_since(window.started_at) >= WINDOW {
            *window = MinuteWindow {
                started_at: now,
                used: 0,
            };
        }

        if window.used >= limit {
            return None;
        }

        window.used += 1;
        Some(MinuteReservation {
            api_key_id,
            window_started_at: window.started_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> Uuid {
        Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap()
    }

    #[test]
    fn accepts_up_to_configured_minute_limit() {
        let limiter = ApiKeyMinuteLimiter::default();
        let now = Instant::now();

        assert!(limiter.reserve_at(key(), 2, now).is_some());
        assert!(limiter.reserve_at(key(), 2, now).is_some());
        assert!(limiter.reserve_at(key(), 2, now).is_none());
    }

    #[test]
    fn resets_after_sixty_seconds() {
        let limiter = ApiKeyMinuteLimiter::default();
        let now = Instant::now();

        assert!(limiter.reserve_at(key(), 1, now).is_some());
        assert!(limiter.reserve_at(key(), 1, now).is_none());
        assert!(limiter
            .reserve_at(key(), 1, now + Duration::from_secs(60))
            .is_some());
    }

    #[test]
    fn rejects_zero_or_negative_limits() {
        let limiter = ApiKeyMinuteLimiter::default();
        let now = Instant::now();

        assert!(limiter.reserve_at(key(), 0, now).is_none());
        assert!(limiter.reserve_at(key(), -1, now).is_none());
    }

    #[test]
    fn release_returns_a_reserved_slot() {
        let limiter = ApiKeyMinuteLimiter::default();
        let now = Instant::now();

        let reservation = limiter.reserve_at(key(), 1, now).unwrap();
        assert!(limiter.reserve_at(key(), 1, now).is_none());

        limiter.release(reservation);

        assert!(limiter.reserve_at(key(), 1, now).is_some());
    }
}
