use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::copy;

mod urls;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    lazy_static! {
    static ref FILTER_TRAPL_URLS: Regex =
        //Regex::new(r###"((http)s?:\\/\\/www.traplinked.com\\/([^\\/]+))?"###).unwrap();
        Regex::new(r###".*traplinked.*"###).unwrap();
    }

    // Read all files in some subdir.
    let paths = fs::read_dir("./pages").unwrap().map(|p| p.unwrap().path());

    let mut map = HashMap::new();

    // Crawl html files.
    for path in paths {
        let file = fs::read_to_string(&path).unwrap();
        let urls = get_links_from(&file);

        let key = path.file_name().unwrap().to_str().unwrap().to_string();

        // Filter on the results, again.
        let urls = filter_regex(&urls, &FILTER_TRAPL_URLS);

        map.insert(key, urls);
    }

    dbg!(map);

    Ok(())
}

/// Make a new vec which only contains the Strings which match the regex.
pub fn filter_regex(results: &[String], regex: &Regex) -> Vec<String> {
    results
        .iter()
        .filter(|s| regex.is_match(s))
        .cloned()
        .collect()
}

/// Make a vec with the links from the given html.
pub fn get_links_from(text: &str) -> Vec<String> {
    lazy_static! {
        static ref RE: Regex =
            Regex::new(r###"<a[^>]*?href\s*=\s*['|"]([^#\\/].*?)['|"][^>]*?>"###).unwrap();
    }

    RE.captures_iter(text)
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
            get_links_from(&url),
            vec!["www.traplinked.com", "www.chip.de"]
        );
    }

    #[test]
    fn malformed_urls() {
        let url = r###"<a href='www.www.www'> <a>, <a href=www>"###;
        assert_eq!(get_links_from(&url), vec!["www.www.www"]);
    }
}
