use anyhow::Context;

use lazy_static::lazy_static;

use petgraph::dot::{Config, Dot};
use petgraph::graphmap::GraphMap;
use petgraph::*;

use petgraph::visit::Dfs;
use regex::Regex;

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{copy, Write};
use std::path::PathBuf;

use structopt::StructOpt;

mod urls;

lazy_static! {
    static ref URL: Regex =
        Regex::new(r###"<a[^>]*?href\s*=\s*['|"]([^#\\/].*?)['|"][^>]*?>"###).unwrap();
}

lazy_static! {
    static ref TRAPL_PREFIXES: Regex =
        Regex::new(r###"http[s]?://www.traplinked.com/(en/|nl/)?"###).unwrap();
}

lazy_static! {
    static ref FILTER_TRAPL_URLS: Regex = Regex::new(r###".*traplinked.*"###).unwrap();
}

#[derive(Debug, StructOpt)]
struct Opt {
    /// Directory with HTML files.
    #[structopt(short = "d", long, parse(from_os_str))]
    directory: PathBuf,

    /// Output file, default to stdout.
    #[structopt(short = "o", long, parse(from_os_str))]
    output: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::from_args();

    if !opt.directory.is_dir() {
        anyhow::bail!(format!("{} is not a directory", opt.directory.display()));
    }

    // Maps page names to URLs they link to.
    let mut map = HashMap::new();

    // Read all files in given directory.
    let paths = fs::read_dir(opt.directory)
        .unwrap()
        .map(|p| p.unwrap().path());

    // Crawl html files.
    for path in paths {
        let file = fs::read_to_string(&path).unwrap();

        let key = path.file_name().unwrap().to_str().unwrap().to_string();

        let urls = get_urls_from(&file);

        // Filter out all non-traplinked urls
        let urls = filter_regex(&urls, &FILTER_TRAPL_URLS);

        let tags: Vec<_> = urls
            .into_iter()
            .map(|u| filter_prefix(&u, &TRAPL_PREFIXES))
            .map(remove_trailing_slash)
            .filter(|s| is_crawling_leftover(s))
            .collect();

        map.insert(key, tags);
    }

    // Make a petgraph `GraphMap` from the page name -> URLs map.
    let graph = make_page_graph(&map);

    // Generate the output in dot format.
    let result = format!("{:?}", Dot::with_config(&graph, &[Config::EdgeNoLabel]));

    // Save result to output file or write to stdout.
    if let Some(path) = opt.output {
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path)
            .context("Could not open output file for writing")?;
        file.write(result.as_bytes())
            .context(format!("Could not write to {}", path.display()))?;
    } else {
        // print to stdout if no output file requested.
        println!("{}", result);
    };

    let orphans = find_orphans(&graph);

    println!("orphan candidates: {:?}", orphans);

    Ok(())
}

/// Find orphans in the given `graph`.
pub fn find_orphans<'a>(graph: &'a GraphMap<&str, &str, Directed>) -> HashSet<&'a str> {
    // A list of all pages.
    let mut orphans: HashSet<&'a str> = graph.nodes().into_iter().collect();

    // Attempt to visit all pages reachable from index.html.
    let mut dfs = Dfs::new(&graph, "index");

    while let Some(v) = dfs.next(&graph) {
        // All visited pages are reachable, so not orphans.
        orphans.remove(v);
    }

    orphans
}

pub fn make_page_graph(data: &HashMap<String, Vec<String>>) -> GraphMap<&str, &str, Directed> {
    let mut graph = petgraph::graphmap::GraphMap::<&str, &str, Directed>::new();

    for (k, values) in data {
        graph.add_node(k.as_str());
        for value in values {
            graph.add_node(value.as_str());
            graph.add_edge(k.as_str(), value.as_str(), "links");
        }
    }
    graph
}

/// Make a new vec which only contains the Strings matching the regex.
pub fn filter_regex(items: &[String], regex: &Regex) -> Vec<String> {
    items
        .iter()
        .filter(|s| regex.is_match(s))
        .cloned()
        .collect()
}

/// Replace with empty string all matches of `regex` in `text`.
pub fn filter_prefix(text: &str, regex: &Regex) -> String {
    regex.replace(text, "").to_string()
}

/// Remove the trailing slash of `text`, if applicable.
pub fn remove_trailing_slash(mut text: String) -> String {
    if text.ends_with('/') {
        text.pop();
    }
    text
}

/// Checks if `text` is empty or contains a ':'.
/// Call this function after filtering out any other http://, mailto:// or text with trailing slashes.
pub fn is_crawling_leftover(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    if text.contains(':') {
        return false;
    }
    true
}

/// Make a vec with the links from the given html.
pub fn get_urls_from(text: &str) -> Vec<String> {
    URL.captures_iter(text)
        .map(|c| c.get(1).unwrap())
        .map(|m| m.as_str().to_string())
        .collect()
}

/// Download the pages at base_url/{urls}.
pub async fn get_pages(base_url: &str, urls: &[&str]) -> Result<(), anyhow::Error> {
    for url in urls {
        let fname = url.to_string();
        let url = format!("{}{}", base_url, url);
        let response = reqwest::get(url).await?;

        let mut dest = { File::create(fname)? };

        let content = response.text().await?;
        copy(&mut content.as_bytes(), &mut dest)?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn regex_matches_url() {
        let url =
            r###"<a href='www.traplinked.com'>, some other text, <a  href =   "www.chip.de">"###;
        assert_eq!(
            get_urls_from(&url),
            vec!["www.traplinked.com", "www.chip.de"]
        );
    }

    #[test]
    fn malformed_urls() {
        let url = r###"<a href='www.www.www'> <a>, <a href=www>"###;
        assert_eq!(get_urls_from(&url), vec!["www.www.www"]);
    }

    #[test]
    fn filter_prefixes() {
        assert_filter("https://www.traplinked.com/hello", "hello");
        assert_filter("http://www.traplinked.com/thing", "thing");
        assert_filter("http://www.traplinked.com/tag/this", "tag/this");
        assert_filter("http://www.traplinked.com/author/who", "author/who");
    }

    fn assert_filter(text: &str, desired: &str) {
        let actual = filter_prefix(text, &TRAPL_PREFIXES);
        assert_eq!(desired, actual);
    }

    #[test]
    fn filters_regexes() {
        let items = vec![
            "hello@".to_string(),
            "hel!lo".to_string(),
            "hello".to_string(),
        ];
        let results = filter_regex(&items, &Regex::new(r".*@.*").unwrap());
        assert_eq!(results, vec!["hello@".to_string()]);
    }

    #[test]
    fn removes_trailing_slash() {
        assert_eq!(remove_trailing_slash("test/".to_string()), "test");
        assert_eq!(remove_trailing_slash("test".to_string()), "test");
        assert_eq!(remove_trailing_slash("/".to_string()), "");
        assert_eq!(remove_trailing_slash("t/e/s/t/".to_string()), "t/e/s/t");
    }

    #[test]
    fn checks_tag() {
        assert!(!is_crawling_leftover(""));
        assert!(!is_crawling_leftover("mailto:lchereti"));
        assert!(is_crawling_leftover("/author/lchereti"));
    }

    #[test]
    fn makes_graph_map() {
        let mut data = HashMap::new();
        data.insert("a".to_string(), vec!["b".to_string(), "c".to_string()]);
        data.insert("b".to_string(), vec!["c".to_string()]);
        data.insert("c".to_string(), vec![]);

        let graph = make_page_graph(&data);

        println!("{:?}", Dot::with_config(&graph, &[Config::EdgeNoLabel]));

        assert_eq!(graph.edge_count(), 3);
        assert_eq!(graph.node_count(), 3);
        assert!(graph.contains_node("a"));
        assert!(graph.contains_node("b"));
        assert!(graph.contains_node("c"));

        assert!(graph.contains_edge("a", "b"));
        assert!(graph.contains_edge("a", "c"));
        assert!(graph.contains_edge("b", "c"));
    }
}
