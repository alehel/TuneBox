//! Remembering replies for the session, so going back and forth between candidates, or
//! fetching a cover a second time, does not ask the services again.

use crate::client::{WebClient, WebError};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

/// How much the cache holds before it stops taking more: enough for many albums' covers and
/// thumbnails, small beside the rest of the app.
const CAPACITY_BYTES: usize = 64 * 1024 * 1024;

/// A [`WebClient`] that remembers successful replies and "nothing there" (404) answers by URL.
/// Failures that may pass (offline, busy, timeouts) are not remembered, so trying again works.
pub struct CachingClient {
    inner: Arc<dyn WebClient>,
    remembered: Mutex<Remembered>,
}

#[derive(Default)]
struct Remembered {
    /// `None`: the service said there is nothing at that URL.
    replies: HashMap<String, Option<Arc<Vec<u8>>>>,
    bytes: usize,
}

impl CachingClient {
    pub fn new(inner: Arc<dyn WebClient>) -> Self {
        Self { inner, remembered: Mutex::new(Remembered::default()) }
    }
}

impl WebClient for CachingClient {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, WebError> {
        // A poisoned lock only means another request panicked mid-way; the map is still whole.
        let known = self.remembered.lock().unwrap_or_else(PoisonError::into_inner).replies.get(url).cloned();
        match known {
            Some(Some(body)) if body.len() as u64 <= limit => return Ok(body.as_ref().clone()),
            Some(Some(_)) => return Err(WebError::TooLarge(limit)),
            Some(None) => return Err(WebError::NotFound),
            None => {}
        }
        let reply = self.inner.get(url, limit);
        let mut remembered = self.remembered.lock().unwrap_or_else(PoisonError::into_inner);
        match &reply {
            Ok(body) if remembered.bytes.saturating_add(body.len()) <= CAPACITY_BYTES => {
                remembered.bytes = remembered.bytes.saturating_add(body.len());
                remembered.replies.insert(url.to_owned(), Some(Arc::new(body.clone())));
            }
            Err(WebError::NotFound) => {
                remembered.replies.insert(url.to_owned(), None);
            }
            _ => {}
        }
        reply
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::tests::FakeWeb;

    #[test]
    fn asks_once_for_replies_and_404s_but_again_after_failures() {
        let fake = Arc::new(
            FakeWeb::default().reply("https://a/ok", Ok(b"picture")).reply("https://a/busy", Err(WebError::Busy)),
        );
        let cache = CachingClient::new(fake.clone());
        for _ in 0..2 {
            assert_eq!(cache.get("https://a/ok", 100).unwrap(), b"picture");
            assert_eq!(cache.get("https://a/none", 100), Err(WebError::NotFound));
            assert_eq!(cache.get("https://a/busy", 100), Err(WebError::Busy));
        }
        assert_eq!(cache.get("https://a/ok", 3), Err(WebError::TooLarge(3)));
        let asked = fake.asked();
        assert_eq!(asked.iter().filter(|u| u.ends_with("/ok")).count(), 1);
        assert_eq!(asked.iter().filter(|u| u.ends_with("/none")).count(), 1);
        assert_eq!(asked.iter().filter(|u| u.ends_with("/busy")).count(), 2);
    }
}
