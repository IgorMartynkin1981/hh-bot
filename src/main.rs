mod app;
mod mdresume;
mod resume;
mod scraper;
mod session;
mod stealth;
mod types;
mod web;

use anyhow::{Context, Result};
use types::{Config, Credentials, ResumeData};
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    let command = &args[1];
    match command.as_str() {
        "serve" => {
            let config = load_config()?;
            let creds = load_credentials()?;
            let resume = load_resume()?;
            let state = web::WebState {
                config: std::sync::Arc::new(config),
                creds: std::sync::Arc::new(creds),
                resume: std::sync::Arc::new(tokio::sync::RwLock::new(resume)),
            };
            info!("Starting web UI at http://127.0.0.1:8787");
            web::serve(state).await
        }
        "login" => {
            let config = load_config()?;
            let creds = load_credentials()?;
            app::login(&config, &creds).await?;
            Ok(())
        }
        "test-login" => {
            let config = load_config()?;
            app::open_session(&config, &load_credentials()?).await?;
            info!("You are logged in on hh.ru");
            Ok(())
        }
        "resume" => {
            let config = load_config()?;
            let resume = load_resume()?;
            let preview = app::update_resume(&config, &load_credentials()?, &resume).await?;
            info!("Resume updated. Preview text:\n{preview}");
            Ok(())
        }
        "resume-md" => {
            let path = config_dir()
                .join("resume.md");
            let resume = mdresume::load_md(&path)?;
            std::fs::write(
                config_dir().join("resume.json"),
                serde_json::to_vec_pretty(&resume)?,
            )?;
            info!("Parsed {}. resume.json updated.\nPreview:\n{}",
                path.display(),
                resume::ResumeBuilder::render_text(&resume));
            Ok(())
        }
        "upload" => cmd_upload(&args).await,
        "scan" => {
            let config = load_config()?;
            let resume = load_resume()?;
            let vacancies = app::scan(&config, &load_credentials()?, &resume).await?;
            for (i, v) in vacancies.iter().enumerate() {
                info!(
                    "#{} [score {}] {} — {} ({}) {} ",
                    i + 1,
                    v.score,
                    v.title,
                    v.company,
                    v.area,
                    v.salary.clone().unwrap_or_default()
                );
                info!("    {}", v.link);
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&vacancies).unwrap_or_default()
            );
            Ok(())
        }
        "apply" => {
            let config = load_config()?;
            let resume = load_resume()?;
            let results = app::apply(&config, &load_credentials()?, &resume).await?;
            for r in &results {
                info!("{}: {}", r.outcome, r.title);
            }
            Ok(())
        }
        _ => {
            print_usage();
            Ok(())
        }
    }
}

fn print_usage() {
    print!("{USAGE_TEXT}");
}

const USAGE_TEXT: &str = "hh_bot \u{2014} hh.ru automation

USAGE: hh_bot <command>

Commands:
  serve       \u{2014} start the web UI at http://127.0.0.1:8787
  login       \u{2014} login and save session cookie (CLI)
  test-login  \u{2014} check that login works (CLI)
  resume      \u{2014} update resume from resume.json (CLI)
  resume-md   \u{2014} parse resume.md and write resume.json (CLI)
  upload      \u{2014} load a resume file (.md/.markdown/.json), parse it and
              upload it to hh.ru (CLI)
  scan        \u{2014} scan matching vacancies and print them (CLI)
  apply       \u{2014} scan and apply to matching vacancies (CLI)

Config files:
  ~/.config/hh_bot/credentials.json  {\"login\":\"...\",\"password\":\"...\"}
  ~/.config/hh_bot/resume.json       see types::ResumeData
  ~/.config/hh_bot/resume.md         optional markdown resume (parsed by resume-md)
  ~/.config/hh_bot/config.json       optional Config

Requires chromedriver running:  chromedriver --port=9515
";

async fn cmd_upload(args: &[String]) -> Result<()> {
    let config = load_config()?;
    let creds = load_credentials()?;
    let path = args
        .get(2)
        .map_or_else(default_resume_path, std::path::PathBuf::from);
    let resume = mdresume::load_file(&path)?;
    if resume.search_query().trim().is_empty() {
        anyhow::bail!("resume must contain at least one keyword for search");
    }
    info!(
        "resume loaded from {} (title: \"{}\", keywords: [{}])",
        path.display(),
        resume.title,
        resume.keywords.join(", ")
    );
    let preview = app::update_resume(&config, &creds, &resume).await?;
    info!("Resume uploaded to hh.ru. Preview text:\n{preview}");
    Ok(())
}

fn default_resume_path() -> std::path::PathBuf {
    let md = config_dir().join("resume.md");
    if md.exists() {
        md
    } else {
        config_dir().join("resume.json")
    }
}

fn config_dir() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("hh_bot")
}

fn load_config() -> Result<Config> {
    let path = config_dir().join("config.json");
    if path.exists() {
        let bytes = std::fs::read(&path)?;
        Ok(serde_json::from_slice(&bytes)?)
    } else {
        Ok(Config::default())
    }
}

fn load_credentials() -> Result<Credentials> {
    let path = config_dir().join("credentials.json");
    let bytes = std::fs::read(&path)
        .with_context(|| format!("Credentials file not found: {}. Create it with login/password.", path.display()))?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn load_resume() -> Result<ResumeData> {
    let path = config_dir().join("resume.json");
    let bytes = std::fs::read(&path)
        .with_context(|| format!("Resume file not found: {}. Create it.", path.display()))?;
    let data: ResumeData = serde_json::from_slice(&bytes)?;
    if data.search_query().trim().is_empty() {
        anyhow::bail!("resume.json must contain at least one keyword for search");
    }
    Ok(data)
}
