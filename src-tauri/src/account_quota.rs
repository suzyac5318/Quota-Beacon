use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use crate::models::AccountWeeklyQuota;

const SUCCESS_TTL: Duration = Duration::from_secs(5 * 60);
const SIGNED_OUT_TTL: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct CachedQuota {
    expires_at: Instant,
    quota: AccountWeeklyQuota,
}

#[derive(Default)]
pub(crate) struct AccountQuotaState {
    entries: HashMap<String, CachedQuota>,
    generations: HashMap<String, u64>,
}

impl AccountQuotaState {
    pub(crate) fn generation(&self, profile_id: &str) -> u64 {
        self.generations.get(profile_id).copied().unwrap_or(0)
    }

    pub(crate) fn get_fresh(
        &mut self,
        profile_id: &str,
        now: Instant,
    ) -> Option<AccountWeeklyQuota> {
        let fresh = self
            .entries
            .get(profile_id)
            .filter(|entry| now < entry.expires_at)
            .map(|entry| entry.quota.clone());
        if fresh.is_none() {
            self.entries.remove(profile_id);
        }
        fresh
    }

    pub(crate) fn insert_if_current(
        &mut self,
        profile_id: &str,
        generation: u64,
        quota: AccountWeeklyQuota,
        now: Instant,
    ) -> bool {
        if self.generation(profile_id) != generation {
            return false;
        }
        let ttl = match quota.status.as_str() {
            "ok" => Some(SUCCESS_TTL),
            "signed_out" => Some(SIGNED_OUT_TTL),
            _ => None,
        };
        match ttl {
            Some(ttl) => {
                self.entries.insert(
                    profile_id.to_string(),
                    CachedQuota {
                        expires_at: now + ttl,
                        quota,
                    },
                );
            }
            None => {
                self.entries.remove(profile_id);
            }
        }
        true
    }

    pub(crate) fn invalidate(&mut self, profile_id: &str) {
        let generation = self.generation(profile_id).wrapping_add(1);
        self.generations.insert(profile_id.to_string(), generation);
        self.entries.remove(profile_id);
    }

    pub(crate) fn prune(&mut self, profile_ids: &HashSet<String>) {
        self.entries
            .retain(|profile_id, _| profile_ids.contains(profile_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quota(profile_id: &str, status: &str) -> AccountWeeklyQuota {
        AccountWeeklyQuota {
            profile_id: profile_id.into(),
            remaining_percent: (status == "ok").then_some(68.0),
            status: status.into(),
            message: None,
        }
    }

    #[test]
    fn successful_quota_expires_at_exactly_five_minutes() {
        let now = Instant::now();
        let mut state = AccountQuotaState::default();
        assert!(state.insert_if_current("profile", 0, quota("profile", "ok"), now));
        assert!(state
            .get_fresh("profile", now + Duration::from_secs(299))
            .is_some());
        assert!(state
            .get_fresh("profile", now + Duration::from_secs(300))
            .is_none());
    }

    #[test]
    fn transient_failures_are_not_cached() {
        let now = Instant::now();
        let mut state = AccountQuotaState::default();
        assert!(state.insert_if_current("profile", 0, quota("profile", "unavailable"), now));
        assert!(state.get_fresh("profile", now).is_none());
    }

    #[test]
    fn invalidation_rejects_an_old_credential_response() {
        let now = Instant::now();
        let mut state = AccountQuotaState::default();
        let generation = state.generation("profile");
        state.invalidate("profile");
        assert!(!state.insert_if_current("profile", generation, quota("profile", "ok"), now));
        assert!(state.get_fresh("profile", now).is_none());
    }
}
