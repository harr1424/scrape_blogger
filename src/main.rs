use fs2::FileExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use regex::Regex;
use reqwest::blocking::get;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashSet;
use std::fs::{self, File};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

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
    let base_url = "https://gnosticesotericstudyworkaids.blogspot.com/";

    let search_timer = Instant::now();
    let post_links: HashSet<String> = scrape_all_posts(base_url)?.par_iter().cloned().collect();

    let mp = MultiProgress::new();
    let pb = mp.add(ProgressBar::new(post_links.len() as u64));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")?
            .progress_chars("#>-"),
    );

    let backup = Arc::new(Mutex::new(Vec::new()));
    let progress = Arc::new(pb);

    post_links.par_iter().for_each(|link| {
        progress.set_message(format!("Scraping: {}", link));

        if let Ok(post) = fetch_and_process_post(link) {
            let mut backup = backup.lock().unwrap();
            backup.push(post);
        }

        progress.inc(1);
    });

    progress.finish_with_message("All posts processed!");

    let search_duration = search_timer.elapsed();
    println!("Searching and scraping took {:?}", search_duration);

    let mut backup = Arc::try_unwrap(backup).unwrap().into_inner().unwrap();
    backup.sort_by(|a, b| b.id.cmp(&a.id));
    write_to_file(&backup, "backup.json")?;

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

fn scrape_all_posts(base_url: &str) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let mut all_links = HashSet::new();
    let mut visited_urls = HashSet::new();
    let mut current_url = base_url.to_string();

    let mut button_count = 0;

    loop {
        if visited_urls.contains(&current_url) {
            println!("Already visited URL: {}", current_url);
            break;
        }

        visited_urls.insert(current_url.clone());

        let html = fetch_html(&current_url)?;
        let document = Html::parse_document(&html);

        let links = extract_post_links(&document)?;
        let link_count = links.len();
        all_links.extend(links);
        println!(
            "Found {} links. Older posts has been clicked {} times.",
            link_count, button_count
        );
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

    Ok(all_links)
}

fn fetch_and_process_post(url: &str) -> Result<Post, Box<dyn std::error::Error>> {
    let html = fetch_html(url)?;
    let document = Html::parse_document(&html);

    let title_selector = Selector::parse("title")?;
    let post_body_selector = Selector::parse(".post-body.entry-content")?;
    let date_header_selector = Selector::parse(".date-header")?;
    let post_outer_selector = Selector::parse(".post-outer")?;

    let title = document
        .select(&title_selector)
        .next()
        .ok_or("Title not found")?
        .inner_html();
    let id = extract_id_from_title(&title);

    let content = document
        .select(&post_body_selector)
        .next()
        .ok_or("Post body not found")?
        .text()
        .collect::<Vec<_>>()
        .join(" ");

    let date = document
        .select(&date_header_selector)
        .next()
        .map(|n| n.inner_html());

    let mut images = Vec::new();
    if let Some(post_outer) = document.select(&post_outer_selector).next() {
        let img_selector = Selector::parse("img")?;
        let meta_selector = Selector::parse("meta[itemprop='image_url']")?;

        for img in post_outer.select(&img_selector) {
            if let Some(src) = img.value().attr("src") {
                images.push(src.to_string());
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
