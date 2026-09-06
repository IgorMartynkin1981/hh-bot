use crate::scraper;
use crate::session::{self, Session};
use crate::types::{Config, Credentials, ResumeData};
use anyhow::Result;
use tracing::{error, info};

/// Runs a full session, authenticating via saved cookies or fresh login.
pub async fn open_session(config: &Config, creds: &Credentials) -> Result<Session> {
    let mut sess = session::Session::start(config).await?;
    sess.login(config, creds).await?;
    Ok(sess)
}

pub async fn login(config: &Config, creds: &Credentials) -> Result<()> {
    let mut sess = session::Session::start(config).await?;
    sess.login(config, creds).await?;
    info!("Login done. Session cookie saved.");
    Ok(())
}

pub async fn update_resume(config: &Config, creds: &Credentials, resume: &ResumeData) -> Result<String> {
    let sess = open_session(config, creds).await?;
    let builder = crate::resume::ResumeBuilder::new(&sess.driver);
    builder.fill_from(resume).await?;
    Ok(crate::resume::ResumeBuilder::render_text(resume))
}

pub async fn scan(config: &Config, creds: &Credentials, resume: &ResumeData) -> Result<Vec<scraper::Vacancy>> {
    let sess = open_session(config, creds).await?;
    let scanner = scraper::VacancyScanner::new(&sess.driver);
    let mut vacancies = scanner.search(resume).await?;
    scraper::rank_vacancies(&mut vacancies);
    Ok(vacancies)
}

pub async fn apply(
    config: &Config,
    creds: &Credentials,
    resume: &ResumeData,
) -> Result<Vec<ApplyResult>> {
    let sess = open_session(config, creds).await?;
    let scanner = scraper::VacancyScanner::new(&sess.driver);
    let mut vacancies = scanner.search(resume).await?;
    scraper::rank_vacancies(&mut vacancies);

    let max = config.max_applies_per_run as usize;
    let to_apply: Vec<_> = vacancies
        .into_iter()
        .filter(|v| v.score > 0)
        .take(max)
        .collect();
    info!("Applying to {} matching vacancies...", to_apply.len());

    let mut results = Vec::new();
    for v in to_apply {
        let outcome = match scraper::apply_to_vacancy(
            &sess.driver,
            &v.link,
            std::time::Duration::from_secs(60),
        )
        .await
        {
            Ok(()) => "applied",
            Err(e) => {
                error!("Failed on '{}': {e}", v.title);
                "failed"
            }
        };
        results.push(ApplyResult {
            title: v.title,
            link: v.link,
            outcome: outcome.to_string(),
        });
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
    Ok(results)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ApplyResult {
    pub title: String,
    pub link: String,
    pub outcome: String,
}
