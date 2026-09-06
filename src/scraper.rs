use crate::types::ResumeData;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thirtyfour::prelude::*;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vacancy {
    pub id: String,
    pub title: String,
    pub company: String,
    pub salary: Option<String>,
    pub area: String,
    pub link: String,
    pub snippet: String,
    pub score: i32,
}

impl Vacancy {
    fn relevance(&self, resume: &ResumeData) -> i32 {
        let mut score = 0;
        let title = self.title.to_lowercase();
        let snippet = self.snippet.to_lowercase();
        for kw in &resume.keywords {
            let kw = kw.to_lowercase();
            if title.contains(&kw) {
                score += 3;
            }
            if snippet.contains(&kw) {
                score += 1;
            }
        }
        score
    }
}

const SEARCH_URL: &str = "https://hh.ru/search/vacancy";

pub struct VacancyScanner<'a> {
    driver: &'a WebDriver,
}

impl<'a> VacancyScanner<'a> {
    pub fn new(driver: &'a WebDriver) -> Self {
        Self { driver }
    }

    /// Builds the vacancy search URL for the resume's keywords and region.
    pub fn search_url(resume: &ResumeData) -> String {
        let mut url = format!("{}?text={}", SEARCH_URL, urlencoding(&resume.search_query()));
        if let Some(area) = &resume.area {
            use std::fmt::Write as _;
            let _ = write!(url, "&area={}", urlencoding(area));
        }
        url
    }

    pub async fn search(&self, resume: &ResumeData) -> Result<Vec<Vacancy>> {
        let url = Self::search_url(resume);
        info!("searching vacancies: {url}");
        self.driver.goto(url).await?;
        parse::scrape(self.driver, resume).await
    }
}

mod parse {
    use super::{Vacancy, VACANCY_SELECTORS};
    use crate::types::ResumeData;
    use anyhow::{Context, Result};

    pub async fn scrape(
        driver: &thirtyfour::WebDriver,
        resume: &ResumeData,
    ) -> Result<Vec<Vacancy>> {
        let mut out = Vec::new();
        let items = driver
            .find_all(thirtyfour::By::Css(VACANCY_SELECTORS[0]))
            .await
            .context("vacancy list items not found")?;

        for item in items {
            let mut v = Vacancy {
                id: String::new(),
                title: String::new(),
                company: String::new(),
                salary: None,
                area: String::new(),
                link: String::new(),
                snippet: String::new(),
                score: 0,
            };

            if let Ok(a) = item.find(thirtyfour::By::Css("a[data-qa=\"serp-item__title\"]")).await {
                v.title = a.text().await.unwrap_or_default();
                if let Ok(href) = a.attr("href").await {
                    v.link = href.unwrap_or_default();
                }
                v.id = v.link.rsplit('/').next().unwrap_or_default().to_string();
            }
            if let Ok(c) = item
                .find(thirtyfour::By::Css("a[data-qa=\"vacancy-serp__vacancy-employer\"]"))
                .await
            {
                v.company = c.text().await.unwrap_or_default();
            }
            if let Ok(s) = item
                .find(thirtyfour::By::Css("span[data-qa=\"vacancy-serp__vacancy-compensation\"]"))
                .await
            {
                v.salary = Some(s.text().await.unwrap_or_default());
            }
            if let Ok(snip) = item
                .find(thirtyfour::By::Css("div[data-qa=\"vacancy-serp__vacancy_snippet_responsibility\"]"))
                .await
            {
                v.snippet = snip.text().await.unwrap_or_default();
            }

            if v.link.is_empty() {
                continue;
            }
            v.score = v.relevance(resume);
            out.push(v);
        }
        Ok(out)
    }
}

pub const VACANCY_SELECTORS: &[&str] = &[
    "div[data-qa=\"vacancy-serp__vacancy\"]",
    "div[data-qa=\"vacancy-serp__vacancy vacancy-serp__vacancy_highlighted\"]",
];

fn urlencoding(s: &str) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                let _ = write!(out, "{}", b as char);
            }
            b' ' => out.push('+'),
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

pub fn rank_vacancies(list: &mut [Vacancy]) {
    list.sort_by_key(|v| std::cmp::Reverse(v.score));
}

// Applies a single vacancy and returns its url.
pub async fn apply_to_vacancy(
    driver: &WebDriver,
    link: &str,
    max_wait: Duration,
) -> Result<()> {
    info!("opening vacancy: {link}");
    driver.goto(link).await?;
    crate::stealth::pause(700, 1800).await;

    // Look for the respond button and click it the way a person would.
    let respond = match driver
        .find(thirtyfour::By::Css("a[data-qa=\"vacancy-response-link-top\"]"))
        .await
    {
        Ok(el) => el,
        Err(_) => driver
            .find(thirtyfour::By::Css("a[data-qa=\"vacancy-response-link-bottom\"]"))
            .await
            .context("respond button not found")?,
    };
    crate::stealth::human_click(driver, &respond).await?;

    // If a modal with "send" button appears, confirm it after a human pause.
    let deadline = std::time::Instant::now() + max_wait;
    loop {
        if let Ok(send) = driver
            .find(thirtyfour::By::Css("button[data-qa=\"vacancy-response-submit\"]"))
            .await
        {
            crate::stealth::human_click(driver, &send).await?;
            info!("application submitted");
            break;
        }
        if std::time::Instant::now() > deadline {
            info!("application sent directly (no modal)");
            break;
        }
        crate::stealth::pause(400, 900).await;
    }

    Ok(())
}
