mod helpers;
mod scrapers;

use helpers::{process_post_links, sort_backup};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Instant;

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
struct Post {
    id: Option<String>,
    title: String,
    content: String,
    URL: String,
    date: Option<String>,
    images: HashSet<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    run()?;
    Ok(())
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut error_written = false;
    let mut log_file = helpers::create_log_file()?;
    let base_url = "https://gnosticesotericstudyworkaids.blogspot.com/";
    let search_timer = Instant::now();

    let post_links: HashSet<String> = scrapers::scrape_base_page_post_links(base_url)?;
    let mut backup = process_post_links(&mut error_written, &mut log_file, post_links)?;

    let search_duration = search_timer.elapsed();
    let minutes = search_duration.as_secs() / 60;
    let seconds = search_duration.as_secs() % 60;
    println!("Searching and scraping took {:02}:{:02}", minutes, seconds);

    backup = sort_backup(backup)?;
    let output_file = "recents.json";
    helpers::write_backup_to_file(&backup, output_file)?;
    helpers::check_errs(error_written);
    helpers::print_time();
    Ok(())
}
