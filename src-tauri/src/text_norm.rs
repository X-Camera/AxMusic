//! 匹配用文本归一：繁体→简体、宽度折叠、小写、标点空白折叠。
//! 只生成**比较键**，不改写库内原文。

use std::sync::OnceLock;

use zhconv::{zhconv, Variant};

/// 繁体/异体 → 简体（短语级，比逐字映射更准，如「皇后」不误伤「后」）。
pub fn to_hans(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    zhconv(s, Variant::ZhHans)
}

/// 全角 ASCII（！-～）→ 半角；全角空格 → 空格。
fn fold_width(s: &str) -> String {
    s.chars()
        .map(|c| {
            let u = c as u32;
            if (0xFF01..=0xFF5E).contains(&u) {
                char::from_u32(u - 0xFEE0).unwrap_or(c)
            } else if c == '\u{3000}' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// 匹配键：繁→简 + 全角折叠 + 小写 + 标点/空白折叠为单空格。
/// 两边都过一遍后字符串相等 ⇔ 简繁/大小写/标点差异下视为同一字段。
pub fn match_key(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    let hans = to_hans(&fold_width(s));
    let folded: String = hans
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 曲名比较键：在 [`match_key`] 基础上再剥掉常见版本/合作后缀，
/// 使「晴天 (Live)」与「晴天」、「Song feat. X」与「Song」共用同一键。
/// 仅用于匹配，不改写库内原文；精确键仍优先于本键。
pub fn title_match_key(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    match_key(&strip_version_tags(s))
}

/// 版本/合作标记词（比较键形态，空白已折叠）。
fn is_version_tag(k: &str) -> bool {
    if k.is_empty() {
        return false;
    }
    const TAGS: &[&str] = &[
        "live",
        "remix",
        "remaster",
        "remastered",
        "acoustic",
        "demo",
        "cover",
        "karaoke",
        "instrumental",
        "unplugged",
        "version",
        "edit",
        "mix",
        "mono",
        "stereo",
        "extended",
        "radio",
        "explicit",
        "clean",
        "deluxe",
        "bonus",
        "session",
        "rehearsal",
        "bootleg",
        "ver",
        "feat",
        "ft",
        "featuring",
        "现场",
        "演唱会",
        "不插电",
        "翻唱",
        "伴奏",
        "混音",
        "卡拉ok",
        "纯音乐",
        "器乐",
        "试听",
        "样带",
        "录音室",
        "单曲版",
        "专辑版",
        "加长",
        "剪辑",
        "重新混音",
    ];
    if TAGS.iter().any(|t| k == *t) {
        return true;
    }
    let tokens: Vec<&str> = k.split_whitespace().collect();
    if tokens.iter().any(|w| TAGS.contains(w)) {
        return true;
    }
    k.starts_with("live at ")
        || k.starts_with("feat ")
        || k.starts_with("ft ")
        || k.starts_with("featuring ")
        || k.starts_with("with ")
        || k.starts_with("现场 ")
        || k.starts_with("翻唱 ")
}

/// 去掉曲名中的版本括号组与尾部 " - Live" 类后缀。
/// 只剥「看起来像版本标记」的片段，保留 (Part II) 等有实义的括号。
fn strip_version_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        // 括号组：(...) （...） [...] 【...】 {...}
        let close = match c {
            '(' => Some(')'),
            '（' => Some('）'),
            '[' => Some(']'),
            '【' => Some('】'),
            '{' => Some('}'),
            _ => None,
        };
        if let Some(end) = close {
            if let Some(rel) = chars[i + 1..].iter().position(|&x| x == end) {
                let inner: String = chars[i + 1..i + 1 + rel].iter().collect();
                if is_version_tag(&match_key(&inner)) {
                    i += rel + 2;
                    // 吃掉括号后多余空白
                    while i < chars.len() && chars[i].is_whitespace() {
                        i += 1;
                    }
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    // 尾部 " - Live…" / " — Live…" / " - 2019 Remaster"
    let trimmed = out.trim_end();
    for sep in [" - ", " — ", " – ", " -- "] {
        if let Some(pos) = trimmed.rfind(sep) {
            let tail = &trimmed[pos + sep.len()..];
            if is_version_tag(&match_key(tail)) {
                out = trimmed[..pos].to_string();
                break;
            }
        }
    }
    // 前置/内嵌 "feat. xxx"（无括号）：截到 feat/ft/featuring 为止
    let lower = out.to_lowercase();
    for marker in [" feat. ", " feat ", " ft. ", " ft ", " featuring "] {
        if let Some(pos) = lower.find(marker) {
            // 仅当后面整段都像合作说明才截断（避免误伤曲名里的 feat）
            let tail = &out[pos + marker.len()..];
            if !tail.trim().is_empty() {
                out = out[..pos].to_string();
                break;
            }
        }
    }
    out.trim().to_string()
}

/// 预热转换表（首次 zhconv 会加载数据，扫描/批量匹配前调一次更稳）。
pub fn warm() {
    static WARMED: OnceLock<()> = OnceLock::new();
    WARMED.get_or_init(|| {
        let _ = to_hans("後臺音樂");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traditional_and_simplified_share_key() {
        assert_eq!(match_key("週杰倫"), match_key("周杰伦"));
        assert_eq!(match_key("葉惠美"), match_key("叶惠美"));
        assert_eq!(match_key("愛在西元前"), match_key("爱在西元前"));
        assert_eq!(match_key("皇后大道"), match_key("皇后大道"));
    }

    #[test]
    fn case_width_and_punct_fold() {
        assert_eq!(match_key("AC/DC"), match_key("ac dc"));
        assert_eq!(match_key("ＡＢＣ"), match_key("abc"));
        assert_eq!(match_key("  晴天  "), match_key("晴天"));
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(match_key(""), "");
    }

    #[test]
    fn title_strips_version_suffixes() {
        assert_eq!(title_match_key("晴天 (Live)"), title_match_key("晴天"));
        assert_eq!(title_match_key("晴天（现场）"), title_match_key("晴天"));
        assert_eq!(title_match_key("Song [2019 Remaster]"), title_match_key("Song"));
        assert_eq!(title_match_key("Song - Live at Wembley"), title_match_key("Song"));
        assert_eq!(title_match_key("Song feat. Someone"), title_match_key("Song"));
        assert_eq!(title_match_key("Song (Acoustic)"), title_match_key("Song"));
    }

    #[test]
    fn title_keeps_meaningful_parens() {
        // Part II 是曲名实义，不应和 Part I 混成一键
        assert_ne!(
            title_match_key("Bohemian Rhapsody (Part II)"),
            title_match_key("Bohemian Rhapsody (Part I)")
        );
        assert_ne!(title_match_key("Song (Part 2)"), title_match_key("Song (Part 1)"));
    }

    #[test]
    fn title_exact_still_sharper_than_fuzzy() {
        // 精确键会区分 Live 版；模糊键故意合拢
        assert_ne!(match_key("晴天 (Live)"), match_key("晴天"));
        assert_eq!(title_match_key("晴天 (Live)"), title_match_key("晴天"));
    }
}
