use serde_json::{json, Value};
use std::sync::LazyLock;

// Umami collector endpoint -- same instance/website used across all our apps,
// just called directly from Rust instead of via the browser tracker script
// (this app has no real routes/pages for that script's History API hooks to
// watch, and calling from the backend means every event fires from the exact
// point it actually happens instead of being reconstructed from frontend state).
const UMAMI_ORIGIN: &str = "https://my-statistics.vercel.app";
const WEBSITE_ID: &str = "cfbcfb4f-22bb-49b0-babe-2391cecde957";

// Umami runs every request's User-Agent through the `isbot` library and
// silently discards events from anything it flags as a bot -- returning a
// fake 200 ({"beep":"boop"}) instead of an error, so a custom app-identifying
// UA (e.g. "YTAudioBar-Desktop") gets flagged and dropped with no visible
// failure. A generic browser-shaped UA passes that check; the real OS/app
// version are already sent explicitly in the event data below.
const TRACKER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

pub struct Analytics {
    install_id: String,
    app_version: String,
    os: &'static str,
}

impl Analytics {
    pub fn new(install_id: String) -> Self {
        Self {
            install_id,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            os: std::env::consts::OS,
        }
    }

    pub fn track(&self, name: &'static str) {
        self.track_with_data(name, Value::Null)
    }

    pub fn track_with_data(&self, name: &'static str, data: Value) {
        let mut fields = json!({
            "install_id": self.install_id,
            "os": self.os,
            "app_version": self.app_version,
        });
        if let (Value::Object(base), Value::Object(extra)) = (&mut fields, data) {
            base.extend(extra);
        }

        let body = json!({
            "type": "event",
            "payload": {
                "website": WEBSITE_ID,
                "url": "/",
                "name": name,
                "data": fields,
                // Umami normally derives a visitor/session id by hashing
                // IP + User-Agent + a rotating daily salt (no cookies). Since
                // every install sends the exact same UA (see above), two users
                // behind the same IP -- same household, office, VPN -- would
                // otherwise collide into a single "visitor" in Umami's native
                // charts. Passing our own persisted install_id as payload.id
                // overrides that default grouping, confirmed via direct API
                // testing: same IP+UA, different id -> different sessionId.
                "id": self.install_id,
            }
        });

        // tauri::async_runtime::spawn (not tokio::spawn) so this can be called from
        // anywhere -- including the raw OS thread the audio player runs on, which
        // isn't a tokio worker and can't use tokio::spawn directly.
        tauri::async_runtime::spawn(async move {
            let result = CLIENT
                .post(format!("{}/api/send", UMAMI_ORIGIN))
                .header("User-Agent", TRACKER_USER_AGENT)
                .timeout(std::time::Duration::from_secs(5))
                .json(&body)
                .send()
                .await;

            // Analytics must never surface an error to the user or affect app
            // behavior -- just note it happened, for our own debugging.
            if let Err(e) = result {
                eprintln!("📊 Analytics send failed (ignored): {}", e);
            }
        });
    }
}
