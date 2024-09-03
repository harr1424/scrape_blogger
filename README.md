## scrape_blogger

```text
Usage: scrape_blogger [OPTIONS]

Options:
  -t, --threads <THREADS>  Sets the number of threads to use when scraping all post links [default: 4]
  -r, --recent-only        Scrapes only recent posts from the blog homepage without clicking 'Older Posts'
  -h, --help               Print help
  -V, --version            Print version
```

Recurisvely crawl and scrape a specific Blogger site in order to archive post content. This project may not generalize well to all Blogger sites. It is hardcoded to work with a specific site, but the source code may be modified to work with any English Blogger site where the site's homepage has a link to older posts. 
