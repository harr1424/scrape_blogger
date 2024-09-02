use super::helpers;
use log::info;
use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::thread;
use std::time::Duration;
use std::time::Instant;

const MAX_RETRIES: u32 = 4;
const RETRY_DELAY: Duration = Duration::from_secs(1);

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct Post {
    id: Option<String>,
    title: String,
    content: String,
    URL: String,
    pub date: Option<String>,
    images: HashSet<String>,
}

pub fn get_recent_posts() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut error_written = false;
    let mut log_file = helpers::create_log_file()?;
    let base_url = "https://gnosticesotericstudyworkaids.blogspot.com/";
    let search_timer = Instant::now();

    let post_links: HashSet<String> = scrape_base_page_post_links(base_url)?;
    let mut backup = helpers::process_post_links(&mut error_written, &mut log_file, post_links)?;

    let search_duration = search_timer.elapsed();
    let minutes = search_duration.as_secs() / 60;
    let seconds = search_duration.as_secs() % 60;
    info!("Searching and scraping took {:02}:{:02}", minutes, seconds);

    backup = helpers::sort_backup(backup)?;
    let output_file = "recents.json";
    helpers::write_backup_to_file(&backup, output_file)?;
    helpers::check_errs(error_written);
    helpers::print_time();
    Ok(())
}

pub fn extract_post_links(
    document: &Html,
) -> Result<HashSet<String>, Box<dyn std::error::Error + Send + Sync>> {
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
) -> Result<HashSet<String>, Box<dyn std::error::Error + Send + Sync>> {
    let html = helpers::fetch_html(base_url)?;
    let document = Html::parse_document(&html);
    extract_post_links(&document)
}

pub fn fetch_and_process_with_retries(
    url: &str,
    logfile: &mut File,
) -> Result<Post, Box<dyn std::error::Error + Send + Sync>> {
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
                    writeln!(
                        logfile,
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

fn fetch_and_process_post(url: &str) -> Result<Post, Box<dyn std::error::Error + Send + Sync>> {
    let html = helpers::fetch_html(url)?;
    let document = Html::parse_document(&html);

    let title_selector = Selector::parse("title").unwrap();
    let date_header_selector = Selector::parse(".date-header").unwrap();
    let post_body_selector = Selector::parse(".post-body.entry-content").unwrap();

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
    if let Some(post_outer) = document
        .select(&Selector::parse(".post-outer").unwrap())
        .next()
    {
        let img_selector = Selector::parse("img").unwrap();
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
