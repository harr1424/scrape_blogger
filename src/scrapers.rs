use super::helpers;
use crate::Cli;
use crate::Post;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use regex::Regex;
use scraper::{Html, Selector};
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const MAX_RETRIES: u32 = 4;
const RETRY_DELAY: Duration = Duration::from_secs(1);

pub fn search_and_scrape(
    args: Cli,
    error_written: Arc<Mutex<bool>>,
    log_file: Arc<Mutex<File>>,
    base_url: &str,
) -> Result<Vec<Post>, Box<dyn std::error::Error>> {
    let post_links: HashSet<String> = if args.recent_only {
        scrape_base_page_post_links(base_url)?
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

    let backup = Arc::try_unwrap(backup).unwrap().into_inner()?;

    Ok(backup)
}

pub fn extract_post_links(document: &Html) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
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

pub fn scrape_base_page_post_links(
    base_url: &str,
) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let html = helpers::fetch_html(base_url)?;
    let document = Html::parse_document(&html);
    extract_post_links(&document)
}

pub fn scrape_all_post_links(
    base_url: &str,
) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
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
        let html = helpers::fetch_html(&current_url)?;
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

        if let Some(next_url) = helpers::find_older_posts_link(&document) {
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

pub fn fetch_and_process_with_retries(
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
    let html = helpers::fetch_html(url)?;
    let document = Html::parse_document(&html);

    let title_selector = Selector::parse("title")?;
    let date_header_selector = Selector::parse(".date-header")?;
    let post_body_selector = Selector::parse(".post-body.entry-content")?;

    let title = document
        .select(&title_selector)
        .next()
        .ok_or("Title not found")?
        .inner_html()
        .replace("Gnostic Esoteric Study &amp; Work Aids: ", "");

    let id = helpers::extract_id_from_title(&title);

    let content = document
        .select(&post_body_selector)
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
        .map(|n| n.text().collect::<Vec<_>>().join(" "));

    let mut images = HashSet::new();
    if let Some(post_outer) = document.select(&Selector::parse(".post-outer")?).next() {
        let img_selector = Selector::parse("img")?;
        //let meta_selector = Selector::parse("meta[itemprop='image_url']")?;

        for img in post_outer.select(&img_selector) {
            if let Some(src) = img.value().attr("src") {
                if !src.contains(".gif") {
                    images.insert(src.to_string());
                }
            }
        }
    }

    Ok(Post {
        id,
        title,
        content,
        URL: url.to_string(),
        date,
        images,
    })
}
