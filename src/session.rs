use crate::stealth;
use crate::types::{Config, Credentials};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::time::Duration;
use thirtyfour::prelude::*;
use tracing::{info, warn};

const HH_LOGIN_URL: &str = "https://hh.ru/account/login";
const HH_MAIN_URL: &str = "https://hh.ru";

pub struct Session {
    pub driver: WebDriver,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct SavedSession {
    cookies: Vec<serde_json::Value>,
}

fn storage_path(config: &Config) -> Result<PathBuf> {
    std::fs::create_dir_all(&config.storage_dir)
        .with_context(|| format!("cannot create dir {}", config.storage_dir.display()))?;
    Ok(config.storage_dir.join("session.json"))
}

/// Chrome leaves `SingletonLock`/`SingletonSocket`/`SingletonCookie` behind on
/// a crash or when a previous driver session was not closed cleanly. A stale
/// lock makes Chrome abort on the next launch, so remove whatever is left.
fn clean_stale_profile_lock(profile_dir: &std::path::Path) {
    for name in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
        let path = profile_dir.join(name);
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                warn!("cannot remove stale {name}: {e}");
            } else {
                info!("removed stale {name} from chrome profile");
            }
        }
    }
}

fn load_saved_cookies(config: &Config) -> Vec<serde_json::Value> {
    let Ok(bytes) = std::fs::read(storage_path(config).unwrap_or_default()) else {
        return Vec::new();
    };
    let saved: SavedSession = serde_json::from_slice(&bytes).unwrap_or_default();
    saved.cookies
}

async fn save_cookies(config: &Config, driver: &WebDriver) -> Result<()> {
    let cookies = driver.get_all_cookies().await?;
    let values: Vec<serde_json::Value> = cookies
        .iter()
        .filter(|c| cookie_json_is_session(c))
        .map(|c| serde_json::to_value(c).unwrap_or_default())
        .collect();
    let saved = SavedSession { cookies: values };
    let path = storage_path(config)?;
    std::fs::write(&path, serde_json::to_vec_pretty(&saved)?)?;
    info!("session cookies saved to {}", path.display());
    Ok(())
}

fn cookie_json_is_session(c: &Cookie) -> bool {
    let json = serde_json::to_value(c).unwrap_or_default();
    json.get("domain")
        .and_then(|d| d.as_str())
        .is_some_and(|d| d.contains("hh.ru"))
}

impl Session {
    pub async fn start(config: &Config) -> Result<Self> {
        // Ensure the persistent profile directory exists.
        std::fs::create_dir_all(&config.profile_dir).with_context(|| {
            format!("cannot create chrome profile dir {}", config.profile_dir.display())
        })?;
        clean_stale_profile_lock(&config.profile_dir);

        info!(
            "launching Chrome with persistent profile {}",
            config.profile_dir.display()
        );
        let caps = stealth::build_chrome_caps(&config.profile_dir, config.headless)?;
        let driver = WebDriver::new("http://localhost:9515", caps)
            .await
            .context(
                "Failed to connect to chromedriver at :9515.\n\
                 Start it with: chromedriver --port=9515",
            )?;

        driver.set_implicit_wait_timeout(Duration::from_secs(10)).await?;

        // Hide automation fingerprints in every page before we navigate.
        if let Err(e) = stealth::inject_stealth(&driver).await {
            warn!("stealth injection failed (continuing anyway): {e:#}");
        }

        driver.goto(HH_MAIN_URL).await?;
        Ok(Self { driver })
    }

    pub async fn login(&mut self, config: &Config, creds: &Credentials) -> Result<()> {
        let saved = load_saved_cookies(config);
        if !saved.is_empty() {
            info!("restoring saved session cookies...");
            if self.try_restore(&saved).await.unwrap_or(false) {
                return Ok(());
            }
            warn!("saved session expired, full login required");
        }

        self.full_login(config, creds).await
    }

    async fn try_restore(&self, cookies: &[serde_json::Value]) -> Result<bool> {
        for cv in cookies {
            if let Ok(cookie) = serde_json::from_value::<Cookie>(cv.clone()) {
                match self.driver.add_cookie(cookie).await {
                    Ok(()) => {}
                    Err(e) => warn!("could not restore cookie: {e}"),
                }
            }
        }
        self.driver.refresh().await?;
        let ok = self.is_logged_in().await.unwrap_or(false);
        if ok {
            info!("session restored from saved cookies");
        }
        Ok(ok)
    }

    async fn full_login(&mut self, config: &Config, creds: &Credentials) -> Result<()> {
        info!("opening login page (same browser window). If a CAPTCHA appears, solve it manually.");
        self.driver.goto(HH_LOGIN_URL).await?;

        // Prefill credentials like a human would (slow typing).
        if let Ok(login) = self
            .driver
            .find(By::Css("input[name=\"login\"]"))
            .await
        {
            stealth::human_fill(&self.driver, &login, &creds.login).await?;
        }

        if let Ok(pass) = self
            .driver
            .find(By::Css("input[name=\"password\"]"))
            .await
        {
            stealth::human_fill(&self.driver, &pass, &creds.password).await?;
        }

        // Submit via human-like click on any of the known submit buttons.
        let submit_selectors = [
            "button[data-qa=\"account-signup-submit\"]",
            "button[type=\"submit\"]",
        ];
        if let Err(e) = stealth::find_and_click(&self.driver, &submit_selectors).await {
            warn!("no submit button found: {e:#}");
        }

        self.wait_for_login().await?;
        if let Err(e) = save_cookies(config, &self.driver).await {
            warn!("failed to save session: {e:#}");
        }
        Ok(())
    }

    async fn wait_for_login(&mut self) -> Result<()> {
        let deadline = std::time::Instant::now() + Duration::from_secs(300);
        while std::time::Instant::now() < deadline {
            if self.is_logged_in().await.unwrap_or(false) {
                info!("login successful");
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        anyhow::bail!("login timeout: check login/password or solve verification manually")
    }

    pub async fn is_logged_in(&self) -> Result<bool> {
        let url = self.driver.current_url().await?;
        if url.as_str().contains("/account/login") {
            return Ok(false);
        }
        let logged = self
            .driver
            .find(By::Css("a[data-qa=\"mainmenu_applicantProfile\"]"))
            .await;
        Ok(logged.is_ok())
    }
}