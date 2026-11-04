use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Duration, Utc};
use rand::random;

const SESSION_HOURS: i64 = 12;
const LOGIN_WINDOW_MINUTES: i64 = 1;
const LOGIN_LIMIT: u32 = 5;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Session {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Default)]
pub struct AuthStore {
    state: Arc<Mutex<AuthState>>,
}

#[derive(Default)]
struct AuthState {
    sessions: HashMap<String, DateTime<Utc>>,
    attempts: HashMap<String, LoginAttempt>,
}

struct LoginAttempt {
    window: DateTime<Utc>,
    count: u32,
}

impl AuthStore {
    pub fn issue(&self) -> Session {
        let now = Utc::now();
        let session = Session {
            token: random_hex_32(),
            expires_at: now + Duration::hours(SESSION_HOURS),
        };
        let mut state = self.state.lock().expect("auth store lock");
        cleanup(&mut state, now);
        state
            .sessions
            .insert(session.token.clone(), session.expires_at);
        session
    }

    pub fn valid(&self, token: &str) -> bool {
        if token.is_empty() {
            return false;
        }
        let now = Utc::now();
        let mut state = self.state.lock().expect("auth store lock");
        cleanup(&mut state, now);
        state
            .sessions
            .get(token)
            .is_some_and(|expires| now < *expires)
    }

    pub fn invalidate_all(&self) {
        self.state.lock().expect("auth store lock").sessions.clear();
    }

    pub fn allow_login(&self, address: &str) -> bool {
        let now = Utc::now();
        let mut state = self.state.lock().expect("auth store lock");
        cleanup(&mut state, now);
        let attempt = state
            .attempts
            .entry(address.into())
            .or_insert(LoginAttempt {
                window: now,
                count: 0,
            });
        attempt.count += 1;
        attempt.count <= LOGIN_LIMIT
    }
}

fn cleanup(state: &mut AuthState, now: DateTime<Utc>) {
    state.sessions.retain(|_, expires| now < *expires);
    state
        .attempts
        .retain(|_, attempt| now - attempt.window < Duration::minutes(LOGIN_WINDOW_MINUTES));
}

fn random_hex_32() -> String {
    let bytes: [u8; 32] = random();
    let mut token = String::with_capacity(64);
    for byte in bytes {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        token.push(char::from(HEX[usize::from(byte >> 4)]));
        token.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    token
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issues_invalidates_and_rate_limits_sessions() {
        let auth = AuthStore::default();
        let session = auth.issue();
        assert_eq!(session.token.len(), 64);
        assert!(auth.valid(&session.token));
        auth.invalidate_all();
        assert!(!auth.valid(&session.token));
        for _ in 0..LOGIN_LIMIT {
            assert!(auth.allow_login("127.0.0.1"));
        }
        assert!(!auth.allow_login("127.0.0.1"));
        assert!(auth.allow_login("127.0.0.2"));
    }
}
