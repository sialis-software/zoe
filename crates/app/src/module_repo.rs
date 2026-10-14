//! The only place the app talks to the network: fetching from the CrossWire repository.
//!
//! This happens only when the person asks in the module manager, and never on its own. All of it
//! goes through [`Fetcher::get`], so the rest of the manager (and its tests) can use a fake.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// A tarball of the `.conf` of every module in the repository.
pub const LIST_URL: &str = "https://www.crosswire.org/ftpmirror/pub/sword/raw/mods.d.tar.gz";

/// Where a module's package (`mods.d/` and `modules/` in a `.zip`) is, or `None` for a name that
/// could not be one (it would change the address).
pub fn package_url(name: &str) -> Option<String> {
    let plain = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')) && !name.starts_with('.');
    plain.then(|| format!("https://www.crosswire.org/ftpmirror/pub/sword/packages/rawzip/{name}.zip"))
}

/// Something that can fetch an address.
pub trait Fetcher: Send + Sync {
    /// Write what `url` serves to `out`, reporting the bytes so far and the total if it is known.
    /// This blocks, so it runs on a thread of its own, and it gives up with `Err` soon after
    /// `cancel` is set.
    fn get(&self, url: &str, out: &mut dyn Write, progress: &mut dyn FnMut(u64, Option<u64>), cancel: &AtomicBool) -> Result<(), String>;
}

/// What a fetch may take, as a guard against something endless.
const MAX_DOWNLOAD: u64 = 4 << 30;

/// The real thing: plain HTTP(S) with `ureq`.
pub struct Http;

impl Fetcher for Http {
    fn get(&self, url: &str, out: &mut dyn Write, progress: &mut dyn FnMut(u64, Option<u64>), cancel: &AtomicBool) -> Result<(), String> {
        let config = ureq::Agent::config_builder()
            .timeout_resolve(Some(Duration::from_secs(15)))
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .build();
        let agent: ureq::Agent = config.into();
        let mut response = agent.get(url).call().map_err(|e| match e {
            ureq::Error::StatusCode(404) => "It is not on the server (404)".to_string(),
            ureq::Error::StatusCode(code) => format!("The server answered with an error ({code})"),
            e => format!("Could not reach the server: {e}"),
        })?;
        let total = response.body().content_length();
        let mut reader = response.body_mut().as_reader();
        let (mut done, mut buffer) = (0u64, vec![0u8; 64 * 1024]);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let n = reader.read(&mut buffer).map_err(|e| format!("The download broke off: {e}"))?;
            if n == 0 {
                return Ok(());
            }
            done += n as u64;
            if done > MAX_DOWNLOAD {
                return Err("That is far bigger than any module".into());
            }
            out.write_all(&buffer[..n]).map_err(|e| format!("Could not save the download: {e}"))?;
            progress(done, total);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_package_is_found_by_its_name_alone() {
        assert_eq!(package_url("KJV").as_deref(), Some("https://www.crosswire.org/ftpmirror/pub/sword/packages/rawzip/KJV.zip"));
        assert!(package_url("StrongsGreek").is_some() && package_url("Pilgrim-1.0").is_some());
        for bad in ["", "../x", "a/b", "a?b", "a b", ".hidden", "a#"] {
            assert_eq!(package_url(bad), None, "{bad}");
        }
    }
}
