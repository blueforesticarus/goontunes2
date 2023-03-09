use linkify::{LinkFinder, LinkKind};
use regex::Regex;
use strum::{Display, EnumString};
use url::Url;

use crate::types::{Link, MusicService};

pub fn extract_urls(content: String) -> Vec<Url> {
    let mut finder = LinkFinder::new();
    finder.url_must_have_scheme(false);
    finder.kinds(&[LinkKind::Url]);
    let links: Vec<_> = finder
        .links(&content)
        .flat_map(|v| Url::parse(v.as_str()))
        .collect();

    links
}

pub fn parse_url(url: Url) -> Option<Link> {
    let mut entry = Link {
        url: url.clone(),
        service: MusicService::Spotify, //dummy
        id: Default::default(),
        kind: None,
    };

    let host = url.host_str().unwrap_or_default();
    if host.contains("youtube") || host.contains("youtu.be") {
        entry.service = MusicService::Youtube;

        //https://regex101.com/r/LeZ9WH/2/
        lazy_static::lazy_static! {
            static ref RE: Regex = Regex::new(r"(?m)(.+?)(/)(watch\x3Fv=)?(embed/watch\x3Ffeature=player_embedded\x26v=)?([a-zA-Z0-9_-]{11})+").unwrap();
        };

        match RE.captures(url.as_str()) {
            Some(v) => {
                // youtube links don't have kind
                entry.id = v.get(5).unwrap().as_str().to_string();
            }
            None => {
                dbg!(url.to_string());
                return None;
            }
        }
    } else if host.contains("spotify") {
        entry.service = MusicService::Spotify;

        //https://regex101.com/r/PvfZk6/1
        lazy_static::lazy_static! {
            static ref RE: Regex = Regex::new(r"(?m)(artist|album|track|user|playlist)/([A-Za-z0-9]+)$").unwrap();
        };

        match RE.captures(url.path()) {
            Some(v) => {
                entry.id = v.get(2).unwrap().as_str().to_string(); //??? should we convert to spotify:<type>:<id> format?
                entry.kind = Some(v.get(1).unwrap().as_str().parse().unwrap());
            }
            None => {
                dbg!(url.to_string());
                return None;
            }
        }
    } else if host.contains("soundcloud") {
        entry.service = MusicService::Soundcloud;
        entry.id = url.path().to_string();
    } else {
        return None;
    }

    Some(entry)
}

#[cfg(test)]
mod tests {
    use crate::utils::links::{extract_urls, parse_url, Link};

    #[test]
    fn url_extract() {
        let txt = "
            http://www.youtube.com/watch?v=iwGFalTRHDA
            http://www.youtube.com/watch?v=iwGFalTRHDA&feature=related
            http://youtu.be/iwGFalTRHDA
            http://youtu.be/n17B_uFF4cA
            http://www.youtube.com/embed/watch?feature=player_embedded&v=r5nB9u4jjy4
            http://www.youtube.com/watch?v=t-ZRX8984sc
            http://youtu.be/t-ZRX8984sc
            https://youtu.be/2sFlFPmUfNo?t=1
            https://play.spotify.com/user/spotifydiscover/playlist/0vL3R9wDeAwmXTTuRATa14
            https://open.spotify.com/track/1TZ3z6TBztuY0TLUlJZ8R7
        "
        .to_string();

        let urls = extract_urls(txt);
        assert_eq!(urls.len(), 10, "{:?}", urls);

        for url in urls {
            let link = parse_url(url.clone());
            assert!(link.is_some(), "invalid {}", url);
        }
    }
}
