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
}
