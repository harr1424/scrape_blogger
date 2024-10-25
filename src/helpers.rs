use crate::Post;
use chrono::NaiveDate;
use fs2::FileExt;
use regex::Regex;
use reqwest::blocking::get;
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub fn create_log_file() -> Result<Arc<Mutex<File>>, Box<dyn std::error::Error>> {
    let log_file = Arc::new(Mutex::new(
        fs::File::create("scrape_blogger.txt").unwrap_or_else(|e| {
            eprintln!("Failed to create log file: {}", e);
            std::process::exit(1);
        }),
    ));
    println!(
        "scrape_blogger.txt log file created successfully at {}",
        env::current_dir()?.display()
    );

    Ok(log_file)
}

pub fn fetch_html(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let response = get(url)?.text()?;
    Ok(response)
}

pub fn find_older_posts_link(document: &Html) -> Option<String> {
    let older_link_selector = Selector::parse("a.blog-pager-older-link").unwrap();

    document
        .select(&older_link_selector)
        .filter_map(|a| a.value().attr("href"))
        .map(String::from)
        .next()
}

pub fn extract_id_from_title(title: &str) -> Option<String> {
    let re = Regex::new(r"\((\d+)\)$").unwrap();
    re.captures(title)
        .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()))
}

pub fn sort_backup(backup: &mut Vec<Post>) -> Result<(), Box<dyn std::error::Error>> {
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

    Ok(())
}

pub fn sort_backup_asc(backup: &mut Vec<&Post>) -> Result<(), Box<dyn std::error::Error>> {
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

        a_date.cmp(&b_date) //asc
    });

    Ok(())
}

pub fn write_to_file(data: &[Post], file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(file_path);
    let file = File::create(path)?;
    file.lock_exclusive()?;
    let json_data = serde_json::to_string_pretty(data)?;
    fs::write(path, json_data)?;
    file.unlock()?;
    println!("Data written to {}", file_path);
    Ok(())
}

pub fn find_duplicates(backup: &[Post], logfile: Arc<Mutex<File>>) {
    print!("Chcking for duplicate post ids...");
    let mut id_counts = HashMap::new();

    for post in backup {
        if let Some(ref id) = post.id {
            *id_counts.entry(id.clone()).or_insert(0) += 1;
        }
    }

    let duplicates: Vec<_> = id_counts.iter().filter(|&(_, &count)| count > 1).collect();

    if duplicates.is_empty() {
        println!("No duplicates found");
    } else {
        println!(
            "{} duplicates found, see log.txt for details",
            duplicates.len()
        );
        let mut log = logfile.lock().unwrap();
        for (id, count) in duplicates {
            writeln!(log, "[DUPLICATE] ID: {} was found {} times", id, count).ok();
        }
    }
}

pub fn find_missing_ids(backup: &[Post], logfile: Arc<Mutex<File>>) -> Result<(), Box<dyn std::error::Error>> {
    println!("Checking for posts with missing ids...");

    let ids: Vec<usize> = backup.iter()
        .filter_map(|post| post.id.as_ref()?.parse::<usize>().ok())
        .collect();

    let num_ids = ids.len();
    let expected_nums: HashSet<_> = (0..=num_ids).collect();
    let actual_ids: HashSet<_> = ids.into_iter().collect();
    let mut missing_ids: Vec<_> = expected_nums.difference(&actual_ids).cloned().collect();

    let mut posts_without_ids: Vec<&Post> = backup.iter().filter(|post| post.id.is_none()).collect();
    sort_backup_asc(&mut posts_without_ids)?;
    missing_ids.sort();

    if missing_ids.is_empty() {
        println!(
            "No missing ids were found. Consecutive ids found from 0 to {}",
            num_ids
        );
    } else {
        println!(
            "{} posts were found to be missing ids, see log.txt for details",
            missing_ids.len()
        );

        let mut log = logfile.lock().unwrap();
        for (missing_id, post) in missing_ids.iter().zip(posts_without_ids.iter()) {
            writeln!(log, "[MISSING] ID: {} may be assigned to post with title {:?}", missing_id, post.title).ok();
        }
    }

    Ok(())
}
