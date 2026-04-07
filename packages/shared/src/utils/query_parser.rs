use regex::Regex;
use std::sync::LazyLock;

static DSL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:(brand|version|country|status|type|players|limit|category|asn|login|flag):(?:"([^"]*)"|'([^']*)'|([^ ]+)))"#).unwrap()
});

pub struct ParsedQuery {
    pub brand: Option<String>,
    pub version: Option<String>,
    pub country: Option<String>,
    pub status: Option<String>,
    pub server_type: Option<String>,
    pub min_players: Option<i32>,
    pub max_players: Option<i32>,
    pub min_max_players: Option<i32>,
    pub max_max_players: Option<i32>,
    pub asn_category: Option<String>,
    pub asn: Option<String>,
    pub login: Option<String>,
    pub flags: Vec<String>,
    pub free_text: Option<String>,
}

pub fn parse(search: &str) -> ParsedQuery {
    let mut brand = None;
    let mut version = None;
    let mut country = None;
    let mut status = None;
    let mut server_type = None;
    let mut min_players = None;
    let mut max_players = None;
    let mut min_max_players = None;
    let mut max_max_players = None;
    let mut asn_category = None;
    let mut asn = None;
    let mut login = None;
    let mut flags = Vec::new();

    let mut remaining = search.to_string();

    for cap in DSL_REGEX.captures_iter(search) {
        let full_match = cap.get(0).unwrap();
        let key = cap.get(1).unwrap().as_str().to_lowercase();
        let val = cap
            .get(2)
            .or(cap.get(3))
            .or(cap.get(4))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();

        // Remove matched token from remaining text
        remaining = remaining.replace(full_match.as_str(), "");

        match key.as_str() {
            "brand" => brand = Some(val),
            "version" => version = Some(val),
            "country" => country = Some(val.to_uppercase()),
            "status" => {
                let v = val.to_lowercase();
                if matches!(v.as_str(), "online" | "offline" | "all") {
                    status = Some(v);
                }
            }
            "type" => {
                let v = val.to_lowercase();
                if matches!(v.as_str(), "java" | "bedrock" | "all") {
                    server_type = Some(v);
                }
            }
            "category" => asn_category = Some(val),
            "asn" => asn = Some(val),
            "login" => login = Some(val.to_lowercase()),
            "flag" => flags.push(val.to_lowercase()),
            "players" => {
                if let Some(stripped) = val.strip_prefix('>') {
                    if let Ok(n) = stripped.parse::<i32>() {
                        min_players = Some(n + 1);
                    }
                } else if let Some(stripped) = val.strip_prefix('<') {
                    if let Ok(n) = stripped.parse::<i32>() {
                        max_players = Some(n - 1);
                    }
                } else if let Some((a, b)) = val.split_once("..") {
                    if !a.is_empty() {
                        min_players = a.parse::<i32>().ok();
                    }
                    if !b.is_empty() {
                        max_players = b.parse::<i32>().ok();
                    }
                } else if let Ok(n) = val.parse::<i32>() {
                    min_players = Some(n);
                    max_players = Some(n);
                }
            }
            "limit" => {
                if let Some(stripped) = val.strip_prefix('>') {
                    if let Ok(n) = stripped.parse::<i32>() {
                        min_max_players = Some(n + 1);
                    }
                } else if let Some(stripped) = val.strip_prefix('<') {
                    if let Ok(n) = stripped.parse::<i32>() {
                        max_max_players = Some(n - 1);
                    }
                } else if let Some((a, b)) = val.split_once("..") {
                    if !a.is_empty() {
                        min_max_players = a.parse::<i32>().ok();
                    }
                    if !b.is_empty() {
                        max_max_players = b.parse::<i32>().ok();
                    }
                } else if let Ok(n) = val.parse::<i32>() {
                    min_max_players = Some(n);
                    max_max_players = Some(n);
                }
            }
            _ => {}
        }
    }

    let free_text = {
        let trimmed = remaining.trim().replace(char::is_whitespace, " ");
        let collapsed = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.is_empty() {
            None
        } else {
            Some(collapsed)
        }
    };

    ParsedQuery {
        brand,
        version,
        country,
        status,
        server_type,
        min_players,
        max_players,
        min_max_players,
        max_max_players,
        asn_category,
        asn,
        login,
        flags,
        free_text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brand_filter() {
        let q = parse("brand:Paper");
        assert_eq!(q.brand, Some("Paper".to_string()));
        assert!(q.free_text.is_none());
    }

    #[test]
    fn test_quoted_value() {
        let q = parse(r#"brand:"Paper Server""#);
        assert_eq!(q.brand, Some("Paper Server".to_string()));
    }

    #[test]
    fn test_players_range() {
        let q = parse("players:5..20");
        assert_eq!(q.min_players, Some(5));
        assert_eq!(q.max_players, Some(20));
    }

    #[test]
    fn test_players_gt() {
        let q = parse("players:>10");
        assert_eq!(q.min_players, Some(11));
        assert!(q.max_players.is_none());
    }

    #[test]
    fn test_players_lt() {
        let q = parse("players:<50");
        assert_eq!(q.max_players, Some(49));
        assert!(q.min_players.is_none());
    }

    #[test]
    fn test_players_exact() {
        let q = parse("players:0");
        assert_eq!(q.min_players, Some(0));
        assert_eq!(q.max_players, Some(0));
    }

    #[test]
    fn test_mixed_with_free_text() {
        let q = parse("brand:Paper version:1.21 some search text");
        assert_eq!(q.brand, Some("Paper".to_string()));
        assert_eq!(q.version, Some("1.21".to_string()));
        assert_eq!(q.free_text, Some("some search text".to_string()));
    }

    #[test]
    fn test_no_dsl_tokens() {
        let q = parse("just plain text");
        assert!(q.brand.is_none());
        assert_eq!(q.free_text, Some("just plain text".to_string()));
    }

    #[test]
    fn test_multiple_filters() {
        let q = parse("brand:Paper country:US status:online type:java category:hosting asn:16509");
        assert_eq!(q.brand, Some("Paper".to_string()));
        assert_eq!(q.country, Some("US".to_string()));
        assert_eq!(q.status, Some("online".to_string()));
        assert_eq!(q.server_type, Some("java".to_string()));
        assert_eq!(q.asn_category, Some("hosting".to_string()));
        assert_eq!(q.asn, Some("16509".to_string()));
    }

    #[test]
    fn test_limit_range() {
        let q = parse("limit:100..500");
        assert_eq!(q.min_max_players, Some(100));
        assert_eq!(q.max_max_players, Some(500));
    }
}
