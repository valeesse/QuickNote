use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(15 * 60);
const MAX_ATTEMPTS_PER_IP: usize = 25;
const MAX_ATTEMPTS_PER_IDENTITY: usize = 10;

#[derive(Default)]
pub struct AuthRateLimiter {
    ip_attempts: HashMap<String, VecDeque<Instant>>,
    identity_attempts: HashMap<String, VecDeque<Instant>>,
}

impl AuthRateLimiter {
    pub fn check(&mut self, ip: &str, identity: &str) -> Option<u64> {
        let now = Instant::now();
        let ip_retry = bucket_retry_after(self.ip_attempts.get_mut(ip), now, MAX_ATTEMPTS_PER_IP);
        let identity_retry = bucket_retry_after(
            self.identity_attempts.get_mut(identity),
            now,
            MAX_ATTEMPTS_PER_IDENTITY,
        );
        cleanup_bucket_map(&mut self.ip_attempts);
        cleanup_bucket_map(&mut self.identity_attempts);
        ip_retry.max(identity_retry)
    }

    pub fn register_failure(&mut self, ip: &str, identity: &str) {
        let now = Instant::now();
        push_attempt(self.ip_attempts.entry(ip.to_string()).or_default(), now);
        push_attempt(
            self.identity_attempts
                .entry(identity.to_string())
                .or_default(),
            now,
        );
        cleanup_bucket_map(&mut self.ip_attempts);
        cleanup_bucket_map(&mut self.identity_attempts);
    }

    pub fn reset_identity(&mut self, identity: &str) {
        self.identity_attempts.remove(identity);
    }
}

fn bucket_retry_after(
    bucket: Option<&mut VecDeque<Instant>>,
    now: Instant,
    limit: usize,
) -> Option<u64> {
    let bucket = bucket?;
    prune_bucket(bucket, now);
    if bucket.len() < limit {
        return None;
    }
    bucket.front().map(|first| {
        WINDOW
            .saturating_sub(now.saturating_duration_since(*first))
            .as_secs()
            + 1
    })
}

fn push_attempt(bucket: &mut VecDeque<Instant>, now: Instant) {
    prune_bucket(bucket, now);
    bucket.push_back(now);
}

fn prune_bucket(bucket: &mut VecDeque<Instant>, now: Instant) {
    while let Some(first) = bucket.front() {
        if now.saturating_duration_since(*first) < WINDOW {
            break;
        }
        bucket.pop_front();
    }
}

fn cleanup_bucket_map(map: &mut HashMap<String, VecDeque<Instant>>) {
    map.retain(|_, bucket| !bucket.is_empty());
}
