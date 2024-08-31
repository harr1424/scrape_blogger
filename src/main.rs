use clap::Parser;
use fs2::FileExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use regex::Regex;
use reqwest::blocking::get;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const MAX_RETRIES: u32 = 4;
const RETRY_DELAY: Duration = Duration::from_secs(1);

#[derive(Parser, Debug)]
#[command(
    author = "John Harrington",
    version = "1.0",
    about = "Blogger Post Scraper"
)]
struct Cli {
    /// Sets the number of threads to use when scraping all post links
    #[arg(short, long, default_value_t = 4)]
    threads: usize,

    /// Scrapes only recent posts from the blog homepage without clicking 'Older Posts'
    #[arg(short, long)]
    recent_only: bool,
}

#[derive(Serialize, Deserialize, Debug)]
struct Post {
    id: Option<String>,
    title: String,
    content: String,
    url: String,
    date: Option<String>,
    images: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();

    let error_written = Arc::new(Mutex::new(false));
    let log_file = Arc::new(Mutex::new(fs::File::create("log.txt")?));

    let base_url = "https://gnosticesotericstudyworkaids.blogspot.com/";
    let search_timer = Instant::now();
    let post_links: HashSet<String> = if args.recent_only {
        scrape_base_page_posts(base_url)?
    } else {
        scrape_all_post_links(base_url)?
    };

    let mp = MultiProgress::new();
    let pb = mp.add(ProgressBar::new(post_links.len() as u64));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")?
            .progress_chars("#>-"),
    );

    let backup = Arc::new(Mutex::new(Vec::new()));
    let progress = Arc::new(pb);

    println!(
        "{} posts were found and will now be scraped",
        post_links.len()
    );

    let pool = ThreadPoolBuilder::new()
        .num_threads(args.threads)
        .build()
        .unwrap();

    pool.install(|| {
        post_links.par_iter().for_each(|link| {
            progress.set_message(format!("Scraping: {}", link));

            match fetch_and_process_with_retries(link, log_file.clone()) {
                Ok(post) => {
                    let mut backup = backup.lock().unwrap();
                    backup.push(post);
                }
                Err(e) => {
                    let mut err_written = error_written.lock().unwrap();
                    *err_written = true;
                    let mut log = log_file.lock().unwrap();
                    writeln!(
                        log,
                        "[ERROR] Failed to scrape post: {} with error: {:?}",
                        link, e
                    )
                    .ok();
                }
            }

            progress.inc(1);
        });
    });

    progress.finish_with_message("All posts processed!");

    let search_duration = search_timer.elapsed();
    let minutes = search_duration.as_secs() / 60;
    let seconds = search_duration.as_secs() % 60;
    println!("Searching and scraping took {:02}:{:02}", minutes, seconds);

    let mut backup = Arc::try_unwrap(backup).unwrap().into_inner().unwrap();
    backup.sort_by(|a, b| {
        let a_id = a.id.as_ref().and_then(|id| id.parse::<isize>().ok());
        let b_id = b.id.as_ref().and_then(|id| id.parse::<isize>().ok());

        a_id.cmp(&b_id).reverse() // reverse for descending
    });

    let output_file = if args.recent_only {
        "recents.json"
    } else {
        "backup.json"
    };
    write_to_file(&backup, output_file)?;

    let error_written = Arc::try_unwrap(error_written)
        .unwrap()
        .into_inner()
        .unwrap();
    if error_written {
        eprintln!("One or more errors ocurred... See log.txt for more information. It may be necessary to re-run using fewer threads");
    } else {
        println!("Be sure to check log.txt for any warnings. If many retries ocurred, you may want to run with fewer threads.")
    }

    find_duplicates(&backup, log_file.clone());
    find_missing_ids(&backup, log_file.clone());

    Ok(())
}

fn fetch_html(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let response = get(url)?.text()?;
    Ok(response)
}

fn extract_post_links(document: &Html) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let div_selector = Selector::parse("div.blog-posts.hfeed").unwrap();
    let a_selector = Selector::parse("a").unwrap();
    let regex =
        Regex::new(r"^https://gnosticesotericstudyworkaids\.blogspot\.com/\d+/.*\.html$").unwrap();

    if let Some(div) = document.select(&div_selector).next() {
        let hrefs = div
            .select(&a_selector)
            .filter_map(|a| a.value().attr("href"))
            .filter(|href| regex.is_match(href))
            .map(String::from)
            .collect::<HashSet<_>>();

        return Ok(hrefs);
    }

    Ok(HashSet::new())
}

fn find_older_posts_link(document: &Html) -> Option<String> {
    let older_link_selector = Selector::parse("a.blog-pager-older-link").unwrap();

    document
        .select(&older_link_selector)
        .filter_map(|a| a.value().attr("href"))
        .map(String::from)
        .next()
}

fn scrape_base_page_posts(base_url: &str) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let html = fetch_html(base_url)?;
    let document = Html::parse_document(&html);
    extract_post_links(&document)
}

fn scrape_all_post_links(base_url: &str) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let mut all_links = HashSet::new();
    let mut current_url = base_url.to_string();

    let mut button_count = 0;

    let progress_bar = ProgressBar::new_spinner();
    progress_bar.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .unwrap(),
    );

    loop {
        let html = fetch_html(&current_url)?;
        let document = Html::parse_document(&html);

        let links = extract_post_links(&document)?;
        all_links.extend(links);

        progress_bar.set_message(format!(
            "Found {} links. Older posts clicked {} times.",
            all_links.len(),
            button_count
        ));
        progress_bar.tick();

        button_count += 1;

        if let Some(next_url) = find_older_posts_link(&document) {
            if next_url == current_url {
                println!("Pagination loop detected: {}", next_url);
                break;
            }
            current_url = next_url;
        } else {
            println!("No 'Older Posts' link found on page");
            break;
        }
    }

    progress_bar.finish_with_message("Initial post link scraping has finished");

    Ok(all_links)
}

fn fetch_and_process_with_retries(
    url: &str,
    logfile: Arc<Mutex<File>>,
) -> Result<Post, Box<dyn std::error::Error>> {
    let mut attempts = 0;

    loop {
        attempts += 1;

        match fetch_and_process_post(url) {
            Ok(post) => {
                return Ok(post);
            }
            Err(e) => {
                if attempts >= MAX_RETRIES {
                    return Err(e);
                } else {
                    let mut log = logfile.lock().unwrap();
                    writeln!(
                        log,
                        "[WARN] Failed to scrape post: {} on attempt {}/{}. Retrying after delay...",
                        url, attempts, MAX_RETRIES
                    )
                    .ok();
                    thread::sleep(RETRY_DELAY);
                }
            }
        }
    }
}

fn fetch_and_process_post(url: &str) -> Result<Post, Box<dyn std::error::Error>> {
    let html = fetch_html(url)?;
    let document = Html::parse_document(&html);

    let title_selector = Selector::parse("title")?;
    let date_header_selector = Selector::parse(".date-header")?;

    let post_body_selectors = vec![
        Selector::parse(".post-outer")?,
        Selector::parse(".post-body.entry-content")?,
        Selector::parse(".post-body")?,
    ];

    let title = document
        .select(&title_selector)
        .next()
        .ok_or("Title not found")?
        .inner_html();

    let id = extract_id_from_title(&title);

    let content = post_body_selectors
        .iter()
        .flat_map(|selector| document.select(selector))
        .filter_map(|element| {
            let text = element.text().collect::<Vec<_>>().join(" ");
            if !text.is_empty() {
                Some(text)
            } else {
                None
            }
        })
        .next()
        .ok_or("Post body not found using any selector")?;

    let date = document
        .select(&date_header_selector)
        .next()
        .map(|n| n.inner_html());

    let mut images = Vec::new();
    if let Some(post_outer) = document.select(&Selector::parse(".post-outer")?).next() {
        let img_selector = Selector::parse("img")?;
        let meta_selector = Selector::parse("meta[itemprop='image_url']")?;

        for img in post_outer.select(&img_selector) {
            if let Some(src) = img.value().attr("src") {
                if !src.contains(".gif") {
                    images.push(src.to_string());
                }
            }
        }

        for meta in post_outer.select(&meta_selector) {
            if let Some(content) = meta.value().attr("content") {
                images.push(content.to_string());
            }
        }
    }

    Ok(Post {
        id,
        title,
        content,
        url: url.to_string(),
        date,
        images,
    })
}

fn extract_id_from_title(title: &str) -> Option<String> {
    let re = Regex::new(r"\((\d+)\)$").unwrap();
    re.captures(title)
        .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()))
}

fn write_to_file(data: &[Post], file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(file_path);
    let file = File::create(path)?;
    file.lock_exclusive()?;
    let json_data = serde_json::to_string_pretty(data)?;
    fs::write(path, json_data)?;
    file.unlock()?;
    println!("Data written to {}", file_path);
    Ok(())
}

fn find_duplicates(backup: &[Post], logfile: Arc<Mutex<File>>) {
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

fn find_missing_ids(backup: &[Post], logfile: Arc<Mutex<File>>) {
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
            writeln!(log, "[MISSING ID] No post having id {} was found", id).ok();
        }
    }
}
