//! Remote resources never enter the WebView directly or inherit native rights.
use crate::files;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::{Notify, Semaphore};

pub struct Access {
    pub approved: AtomicBool,
    closed: AtomicBool,
    cancel: Notify,
    slots: Semaphore,
    requests: AtomicUsize,
    bytes: AtomicUsize,
}
impl Default for Access {
    fn default() -> Self {
        Self {
            approved: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            cancel: Notify::new(),
            slots: Semaphore::new(4),
            requests: AtomicUsize::new(0),
            bytes: AtomicUsize::new(0),
        }
    }
}
impl Access {
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.slots.close();
        self.cancel.notify_waiters();
    }
    pub async fn image(&self, source: &str) -> Result<(Vec<u8>, &'static str), String> {
        let cancelled = self.cancel.notified();
        tokio::pin!(cancelled);
        cancelled.as_mut().enable();
        if !self.approved.load(Ordering::SeqCst) || self.closed.load(Ordering::SeqCst) {
            return Err("Remote-Bilder sind nicht freigegeben.".into());
        }
        if self.requests.fetch_add(1, Ordering::SeqCst) >= 64 {
            return Err("Maximal 64 Remote-Bilder pro Dokumentrevision.".into());
        }
        tokio::select! {
            _ = cancelled => Err("Dokument geschlossen.".into()),
            result = self.download(source) => result,
        }
    }
    async fn download(&self, source: &str) -> Result<(Vec<u8>, &'static str), String> {
        let _permit = self
            .slots
            .acquire()
            .await
            .map_err(|_| "Dokument geschlossen.")?;
        let url = validate_url(source)?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .referer(false)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() > 3
                    || validate_url(attempt.url().as_str()).is_err()
                    || (attempt
                        .previous()
                        .last()
                        .is_some_and(|u| u.scheme() == "https")
                        && attempt.url().scheme() != "https")
                {
                    attempt.error("Weiterleitung nicht erlaubt")
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|_| "Bilddownload nicht verfügbar.")?;
        let mut response = client
            .get(url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|_| "Remote-Bild konnte nicht geladen werden.")?;
        if response
            .content_length()
            .is_some_and(|size| size > files::IMAGE_LIMIT)
        {
            return Err("Remote-Bild überschreitet 16 MiB.".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Bilddownload unterbrochen.")?
        {
            let total = self.bytes.fetch_add(chunk.len(), Ordering::SeqCst);
            if total.saturating_add(chunk.len()) > 64 * 1024 * 1024
                || bytes.len() + chunk.len() > files::IMAGE_LIMIT as usize
            {
                return Err("Downloadlimit überschritten.".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        files::validate_image(bytes)
    }
}

fn validate_url(source: &str) -> Result<url::Url, String> {
    if source.len() > 8192 || source.chars().any(char::is_control) {
        return Err("Ungültige Bildadresse.".into());
    }
    let url = url::Url::parse(source).map_err(|_| "Ungültige Bildadresse.")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Nur HTTP(S)-Bilder ohne Zugangsdaten sind erlaubt.".into());
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urls_do_not_grant_other_protocols_or_credentials() {
        for value in [
            "file:///etc/passwd",
            "data:image/svg+xml,x",
            "https://user:secret@example.com/a",
            "//example.com/a",
            "https://example.com/\nx",
        ] {
            assert!(validate_url(value).is_err(), "{value}");
        }
        assert!(validate_url("https://example.com/a.png?q=1").is_ok());
    }
    #[tokio::test]
    async fn unapproved_and_closed_sessions_never_download() {
        let access = Access::default();
        assert!(access.image("http://127.0.0.1:1/a").await.is_err());
        access.approved.store(true, Ordering::SeqCst);
        access.close();
        assert!(access.image("http://127.0.0.1:1/a").await.is_err());
        assert_eq!(access.requests.load(Ordering::SeqCst), 0);
    }
}
