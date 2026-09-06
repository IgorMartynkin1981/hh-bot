use crate::stealth;
use crate::types::ResumeData;
use anyhow::{Context, Result};
use tracing::{info, warn};

// Functions for building and updating a resume on hh.ru via the web UI.
// The actual selectors on hh.ru change often; we centralize them here.

const RESUME_LIST_URL: &str = "https://hh.ru/applicant/resumes";

pub struct ResumeBuilder<'a> {
    driver: &'a thirtyfour::WebDriver,
}

impl<'a> ResumeBuilder<'a> {
    pub fn new(driver: &'a thirtyfour::WebDriver) -> Self {
        Self { driver }
    }

    /// Opens the resume editor. If the account has no resume yet, creates a new
    /// one; otherwise opens the existing resume form to update its fields.
    pub async fn open_editor(&self) -> Result<()> {
        self.driver.goto(RESUME_LIST_URL).await?;
        stealth::pause(600, 1400).await;

        // No resume yet: click "Create resume" to bring up the form.
        if let Ok(create) = self
            .driver
            .find(thirtyfour::By::Css("a[data-qa=\"resume-create-button\"]"))
            .await
        {
            info!("no resume found, creating a new one");
            stealth::human_click(self.driver, &create).await?;
        }

        // Wait until the resume form (job title input) is on screen.
        self.driver
            .find(thirtyfour::By::Css("input[name=\"title\"]"))
            .await
            .context("resume editor form (input[name=\"title\"]) not found")?;
        info!("resume editor page opened");
        Ok(())
    }

    /// Fills the main text fields of the resume form with the given data.
    pub async fn fill_from(&self, data: &ResumeData) -> Result<()> {
        self.open_editor().await?;

        if !data.title.is_empty() {
            if let Ok(field) = self
                .driver
                .find(thirtyfour::By::Css("input[name=\"title\"]"))
                .await
            {
                stealth::human_fill(self.driver, &field, &data.title).await?;
                info!("set resume title: {}", data.title);
            } else {
                warn!("title field not found");
            }
        }

        if !data.about.is_empty() {
            if let Ok(field) = self
                .driver
                .find(thirtyfour::By::Css("textarea[name=\"about\"]"))
                .await
            {
                stealth::human_fill(self.driver, &field, &data.about).await?;
                info!("set resume description");
            } else {
                warn!("about field not found");
            }
        }

        Ok(())
    }

    /// Composes a human-readable resume text from structured data.
    pub fn render_text(data: &ResumeData) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        if !data.title.is_empty() {
            let _ = writeln!(out, "Желаемая должность: {}", data.title);
        }
        if let Some(sal) = &data.desired_salary {
            let _ = writeln!(out, "Ожидаемая зарплата: {sal}");
        }
        if let Some(area) = &data.area {
            let _ = writeln!(out, "Регион: {area}");
        }
        out.push('\n');
        if !data.about.is_empty() {
            let _ = writeln!(out, "О себе:\n{}", data.about);
        }
        if !data.skills.is_empty() {
            let _ = writeln!(out, "\nНавыки:\n- {}", data.skills.join("\n- "));
        }
        if data.experience_years > 0 {
            let _ = writeln!(out, "\nОпыт работы: {} лет", data.experience_years);
        }
        out
    }
}
