//! 出站请求/路径拼接的输入校验（FIX-17）。
//! id 拼 URL、封面下载、Lucene 查询统一走这里，避免各处手写规则漂移。

/// 数字 id（LRCLIB / 网易云曲目 id）。
pub fn is_numeric_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit())
}

/// 字母数字 token（QQ songmid/albummid 等）。
pub fn is_token_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// MusicBrainz MBID / CAA 资源 id：UUID 形态 `8-4-4-4-12` 十六进制。
pub fn is_mbid(s: &str) -> bool {
    let s = s.trim();
    if s.len() != 36 {
        return false;
    }
    let b = s.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if c != b'-' {
                    return false;
                }
            }
            _ => {
                if !c.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

/// Lucene 查询字段值转义：`\` `"` 及运算符前加反斜杠。
/// 先转义再 percent-encode（`urlencoding`/自写 encode 均可）。
pub fn escape_lucene(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        if matches!(
            c,
            '\\' | '"' | '+' | '-' | '!' | '(' | ')' | ':' | '^' | '[' | ']' | '{' | '}' | '~'
                | '*' | '?' | '|' | '&' | '/'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// 封面下载域名白名单（CAA / iTunes / 网易云 / QQ 图床）。
fn host_allowed(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    const EXACT: &[&str] = &[
        "coverartarchive.org",
        "archive.org",
        "itunes.apple.com",
        "music.126.net",
        "y.gtimg.cn",
        "gtimg.cn",
    ];
    const SUFFIX: &[&str] = &[
        ".archive.org",
        ".mzstatic.com",
        ".music.126.net",
        ".gtimg.cn",
        ".itunes.apple.com",
    ];
    if EXACT.iter().any(|e| h == *e) {
        return true;
    }
    SUFFIX.iter().any(|s| h.ends_with(s))
}

/// 从 `https://host/...` 抽出 host（小写）。拒绝非 https、用户信息、显式端口、IPv6 字面量。
fn https_host(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    let end = rest
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.is_empty() || authority.contains('@') || authority.starts_with('[') {
        return None;
    }
    // 一律拒绝显式端口（默认 443 之外的端口是常见 SSRF 走私面）
    if authority.contains(':') {
        return None;
    }
    if !authority.is_ascii() {
        return None;
    }
    Some(authority)
}

/// 封面 URL 校验：https + 白名单域名 + 无用户信息/端口走私。
pub fn is_allowed_image_url(url: &str) -> bool {
    match https_host(url) {
        Some(h) => host_allowed(h),
        None => false,
    }
}

/// 图片魔数识别（替代 `len>=1024`）。返回 MIME。
pub fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 12 {
        return None;
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.starts_with(b"RIFF") && bytes[8..12] == *b"WEBP" {
        return Some("image/webp");
    }
    // BMP
    if bytes.starts_with(b"BM") {
        return Some("image/bmp");
    }
    None
}

/// Content-Type 是否为图片（含 `image/*` 与常见缺省）。
pub fn content_type_is_image(ct: Option<&str>) -> bool {
    let Some(ct) = ct else {
        return true; // 无头时靠魔数兜底
    };
    let ct = ct.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    ct.is_empty() || ct.starts_with("image/") || ct == "application/octet-stream"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert!(is_numeric_id("12345"));
        assert!(!is_numeric_id("12a"));
        assert!(!is_numeric_id(""));
        assert!(is_token_id("001JD7XY7K5t8Z"));
        assert!(!is_token_id("abc/def"));
        assert!(!is_token_id("a?b=1"));
        assert!(is_mbid("f2ae0b5b-2c3a-4a0e-9c0b-1c2d3e4f5a6b"));
        assert!(!is_mbid("not-a-uuid"));
        assert!(!is_mbid("f2ae0b5b2c3a4a0e9c0b1c2d3e4f5a6b"));
    }

    #[test]
    fn lucene_escape() {
        assert_eq!(escape_lucene(r#"a"b"#), r#"a\"b"#);
        assert_eq!(escape_lucene(r#"a\b"#), r#"a\\b"#);
        assert!(escape_lucene("a*b").contains(r"\*"));
    }

    #[test]
    fn image_url_allowlist() {
        assert!(is_allowed_image_url(
            "https://coverartarchive.org/release/f2ae0b5b-2c3a-4a0e-9c0b-1c2d3e4f5a6b/front-500"
        ));
        assert!(is_allowed_image_url(
            "https://is1-ssl.mzstatic.com/image/thumb/Music/v4/xx/600x600bb.jpg"
        ));
        assert!(is_allowed_image_url("https://y.gtimg.cn/music/photo_new/T002R500x500M000abc.jpg"));
        assert!(!is_allowed_image_url("http://y.gtimg.cn/x.jpg"));
        assert!(!is_allowed_image_url("https://evil.example.com/x.jpg"));
        assert!(!is_allowed_image_url("https://169.254.169.254/latest/meta-data"));
        assert!(!is_allowed_image_url("https://user:pass@coverartarchive.org/x"));
        assert!(!is_allowed_image_url("https://coverartarchive.org:8443/x"));
    }

    #[test]
    fn sniff_mime() {
        let mut jpeg = vec![0u8; 32];
        jpeg[..3].copy_from_slice(&[0xFF, 0xD8, 0xFF]);
        assert_eq!(sniff_image_mime(&jpeg), Some("image/jpeg"));
        assert_eq!(sniff_image_mime(&[0u8; 32]), None);
        assert_eq!(sniff_image_mime(b"<html>error</html>xxxx"), None);
    }
}
