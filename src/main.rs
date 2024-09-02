mod helpers;
mod scrapers;

use chrono::NaiveDate;
use clap::Parser;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;
use std::fs::{self};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[allow(unused_imports)]
use std::process;

#[derive(Parser, Debug)]
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

    #[cfg(not(debug_assertions))]
    {
        let exe_path = env::current_exe()?;
        let exe_dir = exe_path
            .parent()
            .ok_or("Failed to get the directory of the binary")?;

        env::set_current_dir(&exe_dir).unwrap_or_else(|err| {
            eprintln!("Failed to set current directory: {}", err);
            process::exit(1);
        });
    }
    let error_written = Arc::new(Mutex::new(false));
    let log_file = Arc::new(Mutex::new(fs::File::create("log.txt").unwrap_or_else(
        |e| {
            eprintln!("Failed to create log file: {}", e);
            std::process::exit(1);
        },
    )));
    println!(
        "log.txt file created successfully at {}",
        env::current_dir()?.display()
    );

    let base_url = "https://gnosticesotericstudyworkaids.blogspot.com/";
    let search_timer = Instant::now();
    let post_links: HashSet<String> = if args.recent_only {
        scrapers::scrape_base_page_post_links(base_url)?
    } else {
        scrapers::scrape_all_post_links(base_url)?
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

            match scrapers::fetch_and_process_with_retries(link, log_file.clone()) {
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

    if args.recent_only {
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

            b_date.cmp(&a_date) // reverse for descending
        });
    } else {
        backup.sort_by(|a, b| {
            let a_id = a.id.as_ref().and_then(|id| id.parse::<isize>().ok());
            let b_id = b.id.as_ref().and_then(|id| id.parse::<isize>().ok());

            a_id.cmp(&b_id).reverse() // reverse for descending
        });
    }

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
