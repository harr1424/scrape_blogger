mod helpers;
mod scrapers;

use clap::Parser;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Parser, Debug, Clone)]
#[command(
    author = "John Harrington",
    version = "1.0",
    about = "Scrapes posts from a Blogger website"
)]
struct Cli {
    /// Sets the number of threads to use when scraping all post links
    #[arg(short, long, default_value_t = 4)]
    threads: usize,

    /// Scrapes only recent posts from the blog homepage without clicking 'Older Posts'
    #[arg(short, long)]
    recent_only: bool,
}

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
    let args = Cli::parse();
    let error_written = Arc::new(Mutex::new(false));
    let log_file = helpers::create_log_file()?;
    let base_url = "https://gnosticesotericstudyworkaids.blogspot.com/";
    let search_timer = Instant::now();
    let mut backup = scrapers::search_and_scrape(
        args.clone(),
        error_written.clone(),
        log_file.clone(),
        base_url,
    )?;
    let search_duration = search_timer.elapsed();
    let minutes = search_duration.as_secs() / 60;
    let seconds = search_duration.as_secs() % 60;
    println!("Searching and scraping took {:02}:{:02}", minutes, seconds);
    helpers::sort_backup(&mut backup)?;

    let output_file = if args.recent_only {
        "recents.json"
    } else {
        "backup.json"
    };
    helpers::write_to_file(&backup, output_file)?;

    let error_written = Arc::try_unwrap(error_written)
        .unwrap()
        .into_inner()
        .unwrap();
    if error_written {
        eprintln!("One or more errors ocurred... See log.txt for more information. It may be necessary to re-run using fewer threads");
    } else {
        println!("Be sure to check log.txt for any warnings. If many WARNS occurred, you may want to run with fewer threads.");
        println!("If no WARNS occurred, you may try increasing the thread pool using -t <num_threads> to speed things up.")
    }

    if !args.recent_only {
        helpers::find_duplicates(&backup, log_file.clone());
        helpers::find_missing_ids(&backup, log_file.clone());
    }

    Ok(())
}
