use crate::Post;
use fs2::FileExt;
use regex::Regex;
use reqwest::blocking::get;
use scraper::{Html, Selector};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

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

pub fn find_missing_ids(backup: &[Post], logfile: Arc<Mutex<File>>) {
    print!("Chcking for posts with missing ids...");
    let mut ids = Vec::new();
    let mut num_ids: u64 = 0;

    for post in backup {
        if let Some(ref id) = post.id {
            match id.parse::<u64>() {
                Ok(num_id) => {
                    ids.push(num_id);
                    num_ids += 1;
                }
                Err(e) => {
                    eprintln!("Unable to parse post id {}: {}", id, e.to_string())
                }
            }
        }
    }

    let expected_nums: Vec<u64> = (0..=num_ids).collect();
    let mut missing_ids = Vec::new();

    for num in expected_nums {
        if !ids.contains(&num) {
            missing_ids.push(num);
        }
    }

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
        for id in &missing_ids {
            writeln!(log, "[MISSING] ID: {} was not found", id).ok();
        }
    }
}
