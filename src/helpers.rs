use crate::scrapers::{fetch_and_process_with_retries, Post};
use chrono::NaiveDate;
use chrono::{FixedOffset, Utc};
use fs2::FileExt;
use log::{error, info};
use regex::Regex;
use reqwest::blocking::get;
use std::collections::HashSet;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

pub fn create_log_file() -> Result<fs::File, Box<dyn std::error::Error + Send + Sync>> {
    let file = fs::File::create("scrape_blogger_log.txt").map_err(|e| {
        error!("Failed to create log file: {}", e);
        Box::new(e) as Box<dyn std::error::Error + Send + Sync>
    })?;

    info!(
        "log.txt file created successfully at {}",
        env::current_dir()?.display()
    );

    Ok(file)
}

pub fn fetch_html(url: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let response = get(url)?.text()?;
    Ok(response)
}

pub fn extract_id_from_title(title: &str) -> Option<String> {
    let re = Regex::new(r"\((\d+)\)$").unwrap();
    re.captures(title)
        .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()))
}

pub fn process_post_links(
    error_written: &mut bool,
    log_file: &mut File,
    post_links: HashSet<String>,
) -> Result<Vec<Post>, Box<dyn std::error::Error + Send + Sync>> {
    let mut backup = Vec::new();
    info!(
        "{} posts were found and will now be scraped",
        post_links.len()
    );

    post_links.iter().for_each(
        |link| match fetch_and_process_with_retries(link, log_file) {
            Ok(post) => {
                backup.push(post);
            }
            Err(e) => {
                *error_written = true;
                writeln!(
                    log_file,
                    "[ERROR] Failed to scrape post: {} with error: {:?}",
                    link, e
                )
                .ok();
            }
        },
    );

    Ok(backup)
}

pub fn sort_backup(
    mut backup: Vec<Post>,
) -> Result<Vec<Post>, Box<dyn std::error::Error + Send + Sync>> {
    let re = Regex::new(r"(\d{1,2} \w+ \d{4})").unwrap();

    backup.sort_by(|a, b| {
        let a_date = a.date.as_ref().and_then(|d| {
            re.captures(d)
                .and_then(|cap| NaiveDate::parse_from_str(&cap[1], "%d %B %Y").ok())
        });

        let b_date = b.date.as_ref().and_then(|d| {
            re.captures(d)
                .and_then(|cap| NaiveDate::parse_from_str(&cap[1], "%d %B %Y").ok())
        });

        b_date.cmp(&a_date) //desc
    });

    Ok(backup)
}

pub fn write_backup_to_file(
    data: &[Post],
    file_path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path = Path::new(file_path);
    let file = File::create(path)?;
    file.lock_exclusive()?;
    let json_data = serde_json::to_string_pretty(data)?;
    fs::write(path, json_data)?;
    file.unlock()?;
    info!("Data written to {}", file_path);
    Ok(())
}

pub fn check_errs(error_written: bool) {
    if error_written {
        error!("One or more errors ocurred... See log.txt for more information. It may be necessary to re-run using fewer threads");
    }
}

pub fn print_time() {
    let utc_now = Utc::now();
    let offset = FixedOffset::west_opt(6 * 3600);

    match offset {
        Some(fixed_offset) => {
            let local_time = utc_now.with_timezone(&fixed_offset);
            info!("Finished at {local_time}")
        }
        None => {
            error!("Unable to determine offset and print timestamp");
        }
    }
}
