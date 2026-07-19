use serde_json::{json, Value};
use std::sync::LazyLock;

// Umami collector endpoint -- same instance/website used across all our apps,
// just called directly from Rust instead of via the browser tracker script
// (this app has no real routes/pages for that script's History API hooks to
// watch, and calling from the backend means every event fires from the exact
// point it actually happens instead of being reconstructed from frontend state).
const UMAMI_ORIGIN: &str = "https://my-statistics.vercel.app";
const WEBSITE_ID: &str = "cfbcfb4f-22bb-49b0-babe-2391cecde957";

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
            }
        });

        // tauri::async_runtime::spawn (not tokio::spawn) so this can be called from
        // anywhere -- including the raw OS thread the audio player runs on, which
        // isn't a tokio worker and can't use tokio::spawn directly.
        tauri::async_runtime::spawn(async move {
            let result = CLIENT
                .post(format!("{}/api/send", UMAMI_ORIGIN))
                .header("User-Agent", "YTAudioBar-Desktop")
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
