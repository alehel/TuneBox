//! The real client: HTTPS through ureq, polite and bounded.
//!
//! - Every request names TuneBox in its user agent.
//! - Requests to musicbrainz.org are at least [`MUSICBRAINZ_SPACING`] apart, across the whole
//!   process, as MusicBrainz asks of applications (about one a second).
//! - A request gives up after [`CONNECT_TIMEOUT`] to connect and [`OVERALL_TIMEOUT`] in all,
//!   so a slow archive.org server ends in a message, never a hang.
//! - Redirects are followed (the Cover Art Archive hands pictures out from archive.org), at
//!   most [`MAX_REDIRECTS`] of them.

use crate::client::{WebClient, WebError};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

const MUSICBRAINZ_SPACING: Duration = Duration::from_millis(1100);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const OVERALL_TIMEOUT: Duration = Duration::from_secs(40);
const MAX_REDIRECTS: u32 = 5;

/// When the last request to musicbrainz.org was sent, shared by every client in the process.
static LAST_MUSICBRAINZ_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

/// HTTPS requests to the MetaBrainz services.
pub struct HttpClient {
    agent: ureq::Agent,
}

impl HttpClient {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .user_agent(crate::USER_AGENT)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_global(Some(OVERALL_TIMEOUT))
            .max_redirects(MAX_REDIRECTS)
            .https_only(true)
            .build()
            .new_agent();
        Self { agent }
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl WebClient for HttpClient {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, WebError> {
        // Held while waiting and sending, so two jobs cannot both decide it is their turn.
        let _turn = if url.starts_with(crate::MUSICBRAINZ) {
            let mut last = LAST_MUSICBRAINZ_REQUEST.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(wait) = last.map(|at| MUSICBRAINZ_SPACING.saturating_sub(at.elapsed())) {
                std::thread::sleep(wait);
            }
            *last = Some(Instant::now());
            Some(last)
        } else {
            None
        };
        log::info!("GET {url}");
        let mut response = self.agent.get(url).call().map_err(web_error)?;
        response.body_mut().with_config().limit(limit).read_to_vec().map_err(web_error)
    }
}

fn web_error(error: ureq::Error) -> WebError {
    match error {
        ureq::Error::StatusCode(404 | 410) => WebError::NotFound,
        ureq::Error::StatusCode(429 | 503) => WebError::Busy,
        ureq::Error::StatusCode(code) => WebError::Status(code),
        ureq::Error::Timeout(_) => WebError::TimedOut,
        ureq::Error::BodyExceedsLimit(limit) => WebError::TooLarge(limit),
        ureq::Error::HostNotFound => WebError::Offline("the server's name could not be looked up".into()),
        ureq::Error::ConnectionFailed => WebError::Offline("the connection failed".into()),
        ureq::Error::TooManyRedirects | ureq::Error::RedirectFailed => {
            WebError::BadReply("it redirected too often or to an address TuneBox does not follow".into())
        }
        ureq::Error::Io(e) if e.kind() == std::io::ErrorKind::TimedOut => WebError::TimedOut,
        ureq::Error::Io(e) => WebError::Offline(e.to_string()),
        other => WebError::Offline(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_failure_becomes_a_message_the_listener_can_act_on() {
        assert_eq!(web_error(ureq::Error::StatusCode(404)), WebError::NotFound);
        assert_eq!(web_error(ureq::Error::StatusCode(503)), WebError::Busy);
        assert_eq!(web_error(ureq::Error::StatusCode(429)), WebError::Busy);
        assert_eq!(web_error(ureq::Error::StatusCode(500)), WebError::Status(500));
        assert_eq!(web_error(ureq::Error::BodyExceedsLimit(5)), WebError::TooLarge(5));
        assert!(matches!(web_error(ureq::Error::HostNotFound), WebError::Offline(_)));
        assert!(matches!(web_error(ureq::Error::TooManyRedirects), WebError::BadReply(_)));
        let timed_out = std::io::Error::new(std::io::ErrorKind::TimedOut, "slow");
        assert_eq!(web_error(ureq::Error::Io(timed_out)), WebError::TimedOut);
    }
}
