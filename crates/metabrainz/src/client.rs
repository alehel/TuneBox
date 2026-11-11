//! The boundary every request crosses: a [`WebClient`] fetches a URL, or says why it could not.

/// Fetches web pages. The real one is `HttpClient`; tests and demos use their own.
pub trait WebClient: Send + Sync {
    /// The body of the reply to a GET of `url`, following redirects. Fails with
    /// [`WebError::TooLarge`] rather than read more than `limit` bytes.
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, WebError>;
}

/// Why a request failed, in words the listener can act on. Each completes a sentence such as
/// "Could not get the cover: …".
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WebError {
    /// The service has nothing at that address (HTTP 404): no such release, or no cover.
    #[error("there is nothing there")]
    NotFound,
    /// HTTP 503 or 429: the service limits how often it is asked, or is overloaded.
    #[error("the service is busy right now. Try again in a minute")]
    Busy,
    #[error("the service could not be reached ({0}). Check the internet connection and try again")]
    Offline(String),
    #[error("the service took too long to answer. Try again later")]
    TimedOut,
    #[error("the reply was larger than {} MB, far more than expected, so it was not used", .0 / 1_000_000)]
    TooLarge(u64),
    #[error("the service answered with an error (HTTP {0}). Try again later")]
    Status(u16),
    #[error("the service sent a reply TuneBox could not read ({0})")]
    BadReply(String),
    #[error("this copy of TuneBox was built without network access")]
    NoNetwork,
}

/// A client for builds without network access: every request fails, saying so.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoNetwork;

impl WebClient for NoNetwork {
    fn get(&self, _url: &str, _limit: u64) -> Result<Vec<u8>, WebError> {
        Err(WebError::NoNetwork)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// A pretend web: fixed replies by URL, and a log of every URL asked for.
    #[derive(Default)]
    pub(crate) struct FakeWeb {
        pub replies: HashMap<String, Result<Vec<u8>, WebError>>,
        pub asked: Mutex<Vec<String>>,
    }

    impl FakeWeb {
        pub fn reply(mut self, url: &str, reply: Result<&[u8], WebError>) -> Self {
            self.replies.insert(url.to_owned(), reply.map(<[u8]>::to_vec));
            self
        }

        pub fn asked(&self) -> Vec<String> {
            self.asked.lock().map(|a| a.clone()).unwrap_or_default()
        }
    }

    impl WebClient for FakeWeb {
        fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, WebError> {
            if let Ok(mut asked) = self.asked.lock() {
                asked.push(url.to_owned());
            }
            let reply = self.replies.get(url).cloned().unwrap_or(Err(WebError::NotFound))?;
            if reply.len() as u64 > limit {
                return Err(WebError::TooLarge(limit));
            }
            Ok(reply)
        }
    }

    #[test]
    fn messages_say_what_to_do() {
        assert!(WebError::Busy.to_string().contains("Try again"));
        assert_eq!(
            WebError::TooLarge(20_000_000).to_string().split(" MB").next(),
            Some("the reply was larger than 20")
        );
        assert!(NoNetwork.get("https://example.org", 1).is_err());
    }
}
