use lazy_static::lazy_static;
use petgraph::dot::{Config, Dot};
use petgraph::graphmap::GraphMap;
use petgraph::*;
use regex::Regex;

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::copy;

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
static ref FILTER_TRAPL_URLS: Regex =
    //Regex::new(r###"((http)s?:\\/\\/www.traplinked.com\\/([^\\/]+))?"###).unwrap();
    Regex::new(r###".*traplinked.*"###).unwrap();
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Read all files in some subdir.
    let paths = fs::read_dir("./pages").unwrap().map(|p| p.unwrap().path());

    let mut map = HashMap::new();

    // Crawl html files.
    for path in paths {
        let file = fs::read_to_string(&path).unwrap();
        let urls = get_urls_from(&file);
        let urls = filter_regex(&urls, &FILTER_TRAPL_URLS);

        let key = path.file_name().unwrap().to_str().unwrap().to_string();

        // Filter out all non-traplinked urls
        let urls = filter_regex(&urls, &FILTER_TRAPL_URLS);

        let tags: Vec<_> = urls
            .into_iter()
            .map(|u| filter_prefix(&u, &TRAPL_PREFIXES))
            .map(remove_trailing_slash)
            .filter(|s| is_crawling_leftover(&s))
            .collect();

        map.insert(key, tags);
    }

    let graph = make_page_graph(&map);

    println!("{:?}", Dot::with_config(&graph, &[Config::EdgeNoLabel]));

    Ok(())
}

pub fn make_page_graph(data: &HashMap<String, Vec<String>>) -> GraphMap<&str, &str, Directed> {
    let mut graph = petgraph::graphmap::GraphMap::<&str, &str, Directed>::new();

    for (k, values) in data {
        graph.add_node(k.as_str());
        for value in values {
            graph.add_node(value.as_str());
            graph.add_edge(value.as_str(), k.as_str(), "links");
        }
    }
    graph
}

/// Make a new vec which only contains the Strings which match the regex.
pub fn filter_regex(items: &[String], regex: &Regex) -> Vec<String> {
    items
        .iter()
        .filter(|s| regex.is_match(s))
        .cloned()
        .collect()
}

pub fn filter_prefix(text: &str, regex: &Regex) -> String {
    regex.replace(text, "").to_string()
}

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
}
