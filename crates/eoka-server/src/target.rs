use crate::eoka::{Page, Result};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub enum Target {
    Index(usize),
    Ref(String),
    Live(LivePattern),
}

#[derive(Debug, Clone)]
pub enum LivePattern {
    Text(String),
    Placeholder(String),
    Role(String),
    Css(String),
    Id(String),
}

impl Target {
    pub fn parse(s: &str) -> Self {
        let s = s.trim();

        let index = s
            .strip_prefix("index:")
            .or_else(|| s.strip_prefix("idx:"))
            .unwrap_or(s)
            .trim();

        let index = index
            .strip_prefix('[')
            .and_then(|v| v.strip_suffix(']'))
            .unwrap_or(index)
            .trim();
        if let Ok(idx) = index.parse::<usize>() {
            return Target::Index(idx);
        }

        if s.starts_with('@') {
            return Target::Ref(s.to_string());
        }

        Target::Live(LivePattern::parse(s))
    }
}

impl LivePattern {
    pub fn parse(s: &str) -> Self {
        if let Some(v) = s.strip_prefix("text:") {
            return LivePattern::Text(v.into());
        }
        if let Some(v) = s.strip_prefix("placeholder:") {
            return LivePattern::Placeholder(v.into());
        }
        if let Some(v) = s.strip_prefix("role:") {
            return LivePattern::Role(v.into());
        }
        if let Some(v) = s.strip_prefix("css:") {
            return LivePattern::Css(v.into());
        }
        if let Some(v) = s.strip_prefix("id:") {
            return LivePattern::Id(v.into());
        }
        LivePattern::Text(s.into())
    }

    fn as_js_args(&self) -> (&'static str, &str) {
        match self {
            LivePattern::Text(v) => ("text", v),
            LivePattern::Placeholder(v) => ("placeholder", v),
            LivePattern::Role(v) => ("role", v),
            LivePattern::Css(v) => ("css", v),
            LivePattern::Id(v) => ("id", v),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct BBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Deserialize)]
pub struct Resolved {
    pub selector: String,
    pub tag: String,
    pub text: String,
    pub found: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub bbox: BBox,
}

const RESOLVE_JS: &str = include_str!("js/resolve.js");

pub async fn resolve(page: &Page, pattern: &LivePattern) -> Result<Resolved> {
    let (t, v) = pattern.as_js_args();
    let js = format!(
        "{}({},{})",
        RESOLVE_JS,
        serde_json::to_string(t).unwrap(),
        serde_json::to_string(v).unwrap()
    );
    page.evaluate(&js).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_index() {
        assert!(matches!(Target::parse("0"), Target::Index(0)));
        assert!(matches!(Target::parse("15"), Target::Index(15)));
        assert!(matches!(Target::parse("  42  "), Target::Index(42)));
        assert!(matches!(Target::parse("[38]"), Target::Index(38)));
        assert!(matches!(Target::parse(" [ 38 ] "), Target::Index(38)));
        assert!(matches!(Target::parse("index:38"), Target::Index(38)));
        assert!(matches!(Target::parse("index: 38"), Target::Index(38)));
        assert!(matches!(Target::parse("idx:38"), Target::Index(38)));
    }

    #[test]
    fn parse_ref() {
        assert!(matches!(Target::parse("@e1"), Target::Ref(ref s) if s == "@e1"));
        assert!(matches!(Target::parse("@e42"), Target::Ref(ref s) if s == "@e42"));
        assert!(matches!(Target::parse("  @e5  "), Target::Ref(ref s) if s == "@e5"));
        assert!(matches!(Target::parse("@foo"), Target::Ref(ref s) if s == "@foo"));
    }

    #[test]
    fn parse_live_prefixed() {
        assert!(matches!(
            Target::parse("text:Submit"),
            Target::Live(LivePattern::Text(_))
        ));
        assert!(matches!(
            Target::parse("placeholder:Email"),
            Target::Live(LivePattern::Placeholder(_))
        ));
        assert!(matches!(
            Target::parse("css:button"),
            Target::Live(LivePattern::Css(_))
        ));
        assert!(matches!(
            Target::parse("id:btn"),
            Target::Live(LivePattern::Id(_))
        ));
        assert!(matches!(
            Target::parse("role:button"),
            Target::Live(LivePattern::Role(_))
        ));
    }

    #[test]
    fn parse_live_unprefixed() {
        assert!(matches!(
            Target::parse("Submit"),
            Target::Live(LivePattern::Text(_))
        ));
        assert!(matches!(
            Target::parse("Click Me"),
            Target::Live(LivePattern::Text(_))
        ));
        assert!(matches!(
            Target::parse("[not-an-index]"),
            Target::Live(LivePattern::Text(_))
        ));
        assert!(matches!(
            Target::parse("index:abc"),
            Target::Live(LivePattern::Text(_))
        ));
        assert!(matches!(
            Target::parse("text:[38]"),
            Target::Live(LivePattern::Text(_))
        ));
    }

    #[test]
    fn parse_preserves_value() {
        if let Target::Live(LivePattern::Text(v)) = Target::parse("Submit Form") {
            assert_eq!(v, "Submit Form");
        } else {
            panic!("Expected Text");
        }

        if let Target::Live(LivePattern::Css(v)) = Target::parse("css:button.primary") {
            assert_eq!(v, "button.primary");
        } else {
            panic!("Expected Css");
        }

        if let Target::Live(LivePattern::Placeholder(v)) = Target::parse("placeholder:Enter email")
        {
            assert_eq!(v, "Enter email");
        } else {
            panic!("Expected Placeholder");
        }
    }

    #[test]
    fn as_js_args() {
        assert_eq!(
            LivePattern::Text("foo".into()).as_js_args(),
            ("text", "foo")
        );
        assert_eq!(
            LivePattern::Placeholder("bar".into()).as_js_args(),
            ("placeholder", "bar")
        );
        assert_eq!(
            LivePattern::Css("div.x".into()).as_js_args(),
            ("css", "div.x")
        );
        assert_eq!(LivePattern::Id("myid".into()).as_js_args(), ("id", "myid"));
        assert_eq!(
            LivePattern::Role("button".into()).as_js_args(),
            ("role", "button")
        );
    }
}
