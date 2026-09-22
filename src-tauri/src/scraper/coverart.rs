//! Cover Art Archive (front cover). Fail fast — never block apply for minutes.

use anyhow::{anyhow, Context, Result};

use super::{rate_limit_wait, user_agent};

/// Fetch front cover JPEG/PNG bytes. Tries two sizes only; short timeout.
pub fn fetch_front_cover(mbid: &str) -> Result<Vec<u8>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(user_agent())
        .timeout(std::time::Duration::from_secs(8))
        .connect_timeout(std::time::Duration::from_secs(4))
        .build()?;

    let urls = [
        format!("https://coverartarchive.org/release/{mbid}/front-500.jpg"),
        format!("https://coverartarchive.org/release-group/{mbid}/front-500.jpg"),
    ];

    for url in urls {
        rate_limit_wait();
        let resp = client
            .get(&url)
            .send()
            .with_context(|| format!("封面请求失败 {url}"))?;
        if resp.status().is_success() {
            let bytes = resp.bytes().context("读取封面失败")?;
            if bytes.len() > 1024 {
                return Ok(bytes.to_vec());
            }
        }
    }
    Err(anyhow!("Cover Art Archive 无可用封面"))
}
