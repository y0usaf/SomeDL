use reqwest::blocking::Client;
use serde_json::Value;
use std::thread;
use std::time::Duration;

/// Look up the album name for a given artist + song using the Genius API.
/// Uses the public (unauthenticated) endpoint by default.
pub fn get_album_by_song(
    client: &Client,
    artist: &str,
    song: &str,
    use_official: bool,
    token: &str,
) -> Option<String> {
    let api_base = if use_official {
        "api.genius.com"
    } else {
        "genius.com/api"
    };

    let search_url = format!("https://{api_base}/search?q={} {}", encode(artist), encode(song));
    let song_result = genius_search(client, &search_url, use_official, token)
        .or_else(|| {
            // Retry once with a dash separator
            thread::sleep(Duration::from_secs(2));
            let url2 = format!("https://{api_base}/search?q={} - {}", encode(artist), encode(song));
            genius_search(client, &url2, use_official, token)
        })?;

    let api_path = song_result
        .pointer("/response/hits/0/result/api_path")
        .and_then(|v| v.as_str())?
        .to_string();

    let song_url = format!("https://{api_base}/{api_path}");
    let song_resp = genius_get(client, &song_url, use_official, token)?;

    // "album": null means it really is a single
    let album_name = song_resp
        .pointer("/response/song/album/name")
        .and_then(|v| v.as_str())?
        .to_string();

    Some(album_name)
}

fn genius_search(client: &Client, url: &str, use_official: bool, token: &str) -> Option<Value> {
    let resp = genius_get(client, url, use_official, token)?;
    let hits = resp.pointer("/response/hits")?.as_array()?;
    if hits.is_empty() {
        return None;
    }
    Some(resp)
}

fn genius_get(client: &Client, url: &str, use_official: bool, token: &str) -> Option<Value> {
    let mut req = client.get(url);
    if use_official && !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {token}"));
    }
    req.send()
        .ok()
        .filter(|r| r.status().is_success())
        .and_then(|r| r.json::<Value>().ok())
}

fn encode(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
                vec![c]
            } else if c == ' ' {
                vec!['%', '2', '0']
            } else {
                let encoded: Vec<char> = format!("%{:02X}", c as u32).chars().collect();
                encoded
            }
        })
        .collect()
}
