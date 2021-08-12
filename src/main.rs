use lazy_static::lazy_static;
use regex::Regex;
use std::fs::{self, File};
use std::io::copy;

mod urls;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    lazy_static! {
        static ref RE: Regex =
            Regex::new(r###"((http)s?:\\/\\/www.traplinked.com\\/([^\\/]+))?"###).unwrap();
    }

    let _base_url = "https://www.traplinked.com/";

    let paths = fs::read_dir("./pages").unwrap();

    let paths = paths.map(|p| p.unwrap().path());

    for path in paths {
        let file = fs::read_to_string(path).unwrap();
        let urls = get_links_from(&file);

        for url in urls {
            let x = RE
                .captures_iter(url)
                .map(|c| c.get(0).unwrap())
                .map(|m| m.as_str());
            dbg!(&x);
        }
    }

    Ok(())
}

pub fn get_links_from(text: &str) -> Vec<&str> {
    lazy_static! {
        static ref RE: Regex =
            Regex::new(r###"<a[^>]*?href\s*=\s*['|"]([^#\\/].*?)['|"][^>]*?>"###).unwrap();
    }

    RE.captures_iter(text)
        .map(|c| c.get(1).unwrap())
        .map(|m| m.as_str())
        .collect()
}

async fn get_traplinked_pages(base_url: &str, urls: &[&str]) -> Result<(), anyhow::Error> {
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
