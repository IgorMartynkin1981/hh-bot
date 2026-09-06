use crate::types::ResumeData;
use anyhow::{bail, Context, Result};

/// Parses a markdown-formatted resume into structured data.
///
/// Supported format:
///
/// ```text
/// # Rust-разработчик
///
/// ## О себе
/// Текст, могущий занимать несколько строк.
///
/// ## Желаемая зарплата
/// 250000 руб
///
/// ## Регион
/// Москва
///
/// ## Навыки
/// - Rust
/// - Tokio
///
/// ## Ключевые слова
/// Rust, Tokio
///
/// ## Опыт работы
/// 5 лет
/// ```
pub fn parse_md(input: &str) -> Result<ResumeData> {
    let mut data = ResumeData::default();

    let mut sections: Vec<Section> = Vec::new();

    // Tokenize into sections.
    for line in input.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ") {
            if data.title.is_empty() && !title.trim().is_empty() {
                data.title = title.trim().to_string();
            } else if let Some(kind) = classify_section(title.trim()) {
                sections.push(Section {
                    kind,
                    lines: Vec::new(),
                });
            }
            continue;
        }
        if let Some(head) = trimmed.strip_prefix("## ") {
            if let Some(kind) = classify_section(head.trim()) {
                sections.push(Section {
                    kind,
                    lines: Vec::new(),
                });
            }
            continue;
        }
        if let Some(sec) = sections.last_mut() {
            if !trimmed.is_empty() {
                sec.lines.push(trimmed.to_string());
            }
        }
    }

    for sec in &sections {
        apply_section(&mut data, sec);
    }

    if data.title.is_empty() {
        bail!("markdown: не найден заголовок '# Должность'");
    }
    Ok(data)
}

#[derive(Debug)]
struct Section {
    kind: SectionKind,
    lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SectionKind {
    About,
    Salary,
    Area,
    Skills,
    Keywords,
    Experience,
}

fn classify_section(heading: &str) -> Option<SectionKind> {
    let h = heading.to_lowercase();
    let head: &str = &h;
    if [
        "о себе",
        "about",
        "описание",
        "summary",
        "profile",
        "резюме-текст",
    ]
    .contains(&head)
    {
        Some(SectionKind::About)
    } else if [
        "зарплата",
        "salary",
        "желаемая зарплата",
        "ожидаемая зарплата",
        "зп",
    ]
    .contains(&head)
    {
        Some(SectionKind::Salary)
    } else if ["регион", "area", "город", "location", "city"].contains(&head) {
        Some(SectionKind::Area)
    } else if ["навыки", "skills", "компетенции", "stack", "технологии"].contains(&head) {
        Some(SectionKind::Skills)
    } else if [
        "ключевые слова",
        "keywords",
        "поиск",
        "search",
        "ключевые слова для поиска",
    ]
    .contains(&head)
    {
        Some(SectionKind::Keywords)
    } else if [
        "опыт",
        "опыт работы",
        "experience",
        "стаж",
        "years",
        "лет работы",
    ]
    .contains(&head)
    {
        Some(SectionKind::Experience)
    } else {
        None
    }
}

fn apply_section(data: &mut ResumeData, sec: &Section) {
    let is_list = sec.lines.len() > 1 || sec.lines.first().is_some_and(|l| l.starts_with('-'));
    match sec.kind {
        SectionKind::About => {
            data.about = sec.lines.join("\n");
        }
        SectionKind::Salary => {
            if let Some(s) = sec.lines.first() {
                data.desired_salary = Some(s.trim_start_matches(": ").to_string());
            }
        }
        SectionKind::Area => {
            if let Some(s) = sec.lines.first() {
                data.area = Some(s.trim().to_string());
            }
        }
        SectionKind::Skills | SectionKind::Keywords => {
            let items: Vec<String> = if is_list {
                sec.lines
                    .iter()
                    .map(|l| l.trim_start_matches(['-', '*', ' ']).trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                sec.lines
                    .join(" ")
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            };
            let target = if sec.kind == SectionKind::Skills {
                &mut data.skills
            } else {
                &mut data.keywords
            };
            target.extend(items);
        }
        SectionKind::Experience => {
            let text = sec.lines.join(" ");
            let years: u32 = text
                .chars()
                .filter(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .unwrap_or(0);
            if years > 0 {
                data.experience_years = years;
            }
        }
    }
}

/// Loads and parses a markdown file from disk.
pub fn load_md(path: &std::path::Path) -> Result<ResumeData> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read markdown file {}", path.display()))?;
    parse_md(&text).with_context(|| format!("failed to parse {}", path.display()))
}

/// Loads a resume file and parses it, guessing the format from the extension:
/// `.md`/`.markdown` go through the markdown parser, `.json` is deserialized as
/// `ResumeData` directly.
pub fn load_file(path: &std::path::Path) -> Result<ResumeData> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "md" | "markdown" => load_md(path),
        "json" => {
            let bytes = std::fs::read(path)
                .with_context(|| format!("cannot read resume file {}", path.display()))?;
            serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse {}", path.display()))
        }
        _ => anyhow::bail!(
            "unsupported resume file format: .{ext} (expected .md, .markdown or .json)"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r"# Rust-разработчик

## О себе
Делаю высоконагруженные сервисы.
Увлекаюсь системами.

## Желаемая зарплата
250000 руб

## Регион
Москва

## Навыки
- Rust
- Tokio
- HTTP

## Ключевые слова
Rust, Tokio, бекенд

## Опыт работы
5 лет
";

    #[test]
    fn parses_full_resume() {
        let d = parse_md(SAMPLE).unwrap();
        assert_eq!(d.title, "Rust-разработчик");
        assert_eq!(d.desired_salary.as_deref(), Some("250000 руб"));
        assert_eq!(d.area.as_deref(), Some("Москва"));
        assert_eq!(d.skills, vec!["Rust", "Tokio", "HTTP"]);
        assert_eq!(d.keywords, vec!["Rust", "Tokio", "бекенд"]);
        assert_eq!(d.experience_years, 5);
        assert!(d.about.contains("Делаю высоконагруженные сервисы."));
    }

    #[test]
    fn requires_title() {
        assert!(parse_md("## О себе\nтекст").is_err());
    }

    #[test]
    fn commas_only_keywords() {
        let d = parse_md("# Т\n## Ключевые слова\nRust,Tokio").unwrap();
        assert_eq!(d.keywords, vec!["Rust", "Tokio"]);
    }

    #[test]
    fn load_file_detects_md_and_json() {
        let dir = std::env::temp_dir().join("hh_bot_md_test");
        std::fs::create_dir_all(&dir).unwrap();
        let md = dir.join("r.md");
        let json = dir.join("r.json");
        std::fs::write(&md, "# Rust-разработчик\n## Навыки\n- Rust\n").unwrap();
        std::fs::write(&json, "{\"title\":\"Rust-разработчик\",\"about\":\"x\",\"skills\":[\"Rust\"],\"keywords\":[\"Rust\"],\"experience_years\":0}").unwrap();

        let from_md = load_file(&md).unwrap();
        assert_eq!(from_md.title, "Rust-разработчик");
        assert_eq!(from_md.skills, vec!["Rust"]);

        let from_json = load_file(&json).unwrap();
        assert_eq!(from_json.title, "Rust-разработчик");

        let bad = dir.join("r.txt");
        std::fs::write(&bad, "whatever").unwrap();
        assert!(load_file(&bad).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}