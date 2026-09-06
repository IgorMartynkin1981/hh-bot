use anyhow::Context;
use rand::Rng;
use std::path::Path;
use std::time::Duration;
use thirtyfour::common::capabilities::chrome::ChromeCapabilities;
use thirtyfour::extensions::cdp::ChromeDevTools;
use thirtyfour::prelude::*;
use tracing::{info, warn};

/// Builds Chrome capabilities that hide the automation from the site:
/// - persistent profile (real browsing state, no test infobars)
/// - `enable-automation` switch excluded (removes the "controlled by automated
///   test software" banner and the automation flag)
/// - blink feature that surfaces `window.navigator.webdriver` disabled
/// - no first-run dialogs
pub fn build_chrome_caps(profile_dir: &Path, headless: bool) -> WebDriverResult<ChromeCapabilities> {
    let mut caps = ChromeCapabilities::new();
    if headless {
        caps.set_headless()?;
    }
    caps.add_arg(&format!("--user-data-dir={}", profile_dir.display()))?;
    caps.add_arg("--disable-blink-features=AutomationControlled")?;
    caps.add_arg("--disable-infobars")?;
    caps.add_arg("--no-first-run")?;
    caps.add_arg("--no-default-browser-check")?;
    caps.add_arg("--disable-popup-blocking")?;
    caps.add_arg("--lang=ru-RU")?;
    caps.add_exclude_switch("enable-automation")?;

    info!("Chrome launch args: {}", caps.args().join(" "));
    Ok(caps)
}

/// Injects a stealth script into every newly created document. It removes the
/// most well-known fingerprint that web-sites use to detect automation.
pub async fn inject_stealth(driver: &WebDriver) -> anyhow::Result<()> {
    let devtools = ChromeDevTools::new(driver.handle.clone());
    let script = STEALTH_SCRIPT;
    let params = serde_json::json!({ "source": script });
    devtools
        .execute_cdp_with_params("Page.addScriptToEvaluateOnNewDocument", params)
        .await
        .context("CDP is not available; stealth injection skipped")?;
    info!("stealth script injected");
    Ok(())
}

pub const STEALTH_SCRIPT: &str = r"
(() => {
  // Hide the WebDriver readiness flag.
  Object.defineProperty(Navigator.prototype, 'webdriver', {
    get: () => undefined,
    configurable: true,
  });
  // Average human ChROME flavor for headless-less user profiles.
  if (!window.chrome) {
    Object.defineProperty(window, 'chrome', {
      value: { runtime: {} },
      configurable: true,
    });
  }
  // Real-ish plugins list.
  Object.defineProperty(Navigator.prototype, 'plugins', {
    get: () => {
      const names = ['PDF Viewer', 'Chrome PDF Viewer', 'Chromium PDF Viewer', 'Microsoft Edge PDF Viewer', 'WebKit built-in PDF'];
      const plugins = [];
      for (let i = 0; i < names.length; i++) {
        plugins.push({ name: names[i], filename: 'internal-pdf-viewer', description: 'Portable Document Format' });
      }
      return plugins;
    },
  });
  // Browser locale: Russian speakers target ru-RU.
  Object.defineProperty(Navigator.prototype, 'languages', {
    get: () => ['ru-RU', 'ru', 'en-US', 'en'],
  });
  // Notifications permission looks like the real one.
  const _query = Navigator.prototype.permissions && Navigator.prototype.permissions.query;
  if (_query) {
    Object.defineProperty(Navigator.prototype.permissions, 'query', {
      get: () => function (parameters) {
        if (parameters && parameters.name === 'notifications') {
          return Promise.resolve({ state: Notification.permission });
        }
        return _query.call(this, parameters);
      },
    });
  }
})();
";

/// Random pause in [`min_ms`, `max_ms`] mimicking human response time.
pub async fn pause(min_ms: u64, max_ms: u64) {
    let delay = {
        let mut rng = rand::rng();
        Duration::from_millis(rng.random_range(min_ms..=max_ms))
    };
    tokio::time::sleep(delay).await;
}

/// Types text character-by-character with human-like variable speed.
pub async fn type_like_human(_driver: &WebDriver, element: &WebElement, text: &str) -> anyhow::Result<()> {
    for ch in text.chars() {
        element.send_keys(ch).await?;
        let delay = {
            let mut rng = rand::rng();
            let base = rng.random_range(30_u64..=90_u64);
            // natural pauses between words
            if ch == ' ' {
                Duration::from_millis(rng.random_range(120..=350))
            } else {
                Duration::from_millis(base)
            }
        };
        tokio::time::sleep(delay).await;
    }
    Ok(())
}

/// Moves the mouse over the element and clicks it the way a person would.
pub async fn human_click(driver: &WebDriver, element: &WebElement) -> anyhow::Result<()> {
    pause(140, 520).await;
    driver
        .action_chain_with_delay(
            Some(Duration::from_millis(80)),
            Some(Duration::from_millis(180)),
        )
        .move_to_element_center(element)
        .click()
        .perform()
        .await
        .context("human click failed")?;
    pause(180, 480).await;
    Ok(())
}

/// Human-like fill for an input already focused: clear + type.
pub async fn human_fill(driver: &WebDriver, element: &WebElement, value: &str) -> anyhow::Result<()> {
    element.click().await?;
    pause(120, 300).await;
    element.clear().await?;
    pause(100, 220).await;
    type_like_human(driver, element, value).await?;
    pause(150, 400).await;
    Ok(())
}

/// Waits for the element at the given CSS selector, with random discarded
/// attempts between pauses to look like a human deciding what to click.
pub async fn find_and_click(driver: &WebDriver, selectors: &[&str]) -> anyhow::Result<WebElement> {
    for selector in selectors {
        pause(200, 700).await;
        if let Ok(el) = driver.find(By::Css((*selector).to_string())).await {
            human_click(driver, &el).await?;
            return Ok(el);
        }
        warn!("selector not found: {selector}");
    }
    anyhow::bail!("none of the selectors found: {selectors:?}")
}