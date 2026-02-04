//! Live element targeting - resolves elements at action time via JS.

use eoka::{Page, Result};
use serde::Deserialize;

/// Target selector - either an index or a live pattern.
#[derive(Debug, Clone)]
pub enum Target {
    /// Element index from cached observe list
    Index(usize),
    /// Live pattern resolved via JS at action time
    Live(LivePattern),
}

/// Live targeting patterns - all resolved via JS injection.
#[derive(Debug, Clone)]
pub enum LivePattern {
    /// `text:Submit` - find by visible text
    Text(String),
    /// `placeholder:Enter code` - find by placeholder
    Placeholder(String),
    /// `role:button` - find by tag/ARIA role
    Role(String),
    /// `css:form button` - direct CSS selector
    Css(String),
    /// `id:submit-btn` - find by ID
    Id(String),
}

impl Target {
    /// Parse target string. Numbers become Index, everything else is Live.
    pub fn parse(s: &str) -> Self {
        let s = s.trim();

        // Numbers are indices
        if let Ok(idx) = s.parse::<usize>() {
            return Target::Index(idx);
        }

        // Everything else is a live pattern
        Target::Live(LivePattern::parse(s))
    }
}

impl LivePattern {
    /// Parse a live pattern. Unprefixed strings default to text search.
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
        // Default: treat as text search
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

/// Bounding box.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct BBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Result from live resolution.
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

const RESOLVE_JS: &str = r#"
((type, value) => {
    const lc = s => (s || '').toLowerCase().trim();
    const valLc = lc(value);

    function selector(el) {
        if (el.id) return '#' + CSS.escape(el.id);
        const path = [];
        let n = el;
        while (n && n.nodeType === 1) {
            let s = n.tagName.toLowerCase();
            if (n.id) { path.unshift('#' + CSS.escape(n.id)); break; }
            const p = n.parentElement;
            if (p) {
                const sibs = [...p.children].filter(c => c.tagName === n.tagName);
                if (sibs.length > 1) s += ':nth-of-type(' + (sibs.indexOf(n) + 1) + ')';
            }
            path.unshift(s);
            n = p;
        }
        return path.join(' > ');
    }

    function text(el) {
        return el.innerText?.trim() || el.value || el.getAttribute('aria-label') || el.title || el.placeholder || '';
    }

    function interactive() {
        return [...document.querySelectorAll('a,button,input,select,textarea,[role="button"],[onclick],[tabindex]')]
            .filter(el => {
                const r = el.getBoundingClientRect();
                const s = getComputedStyle(el);
                return r.width > 0 && r.height > 0 && s.visibility !== 'hidden' && s.display !== 'none';
            });
    }

    let el = null;
    switch (type) {
        case 'text':
            el = interactive().find(e => lc(text(e)).includes(valLc));
            break;
        case 'placeholder':
            el = document.querySelector(`input[placeholder*="${value}" i],textarea[placeholder*="${value}" i]`)
                || interactive().find(e => lc(e.placeholder).includes(valLc));
            break;
        case 'role':
            el = document.querySelector(valLc) || document.querySelector(`[role="${value}"]`)
                || interactive().find(e => e.tagName.toLowerCase() === valLc || e.getAttribute('role') === value);
            break;
        case 'css':
            el = document.querySelector(value);
            break;
        case 'id':
            el = document.getElementById(value);
            break;
    }

    if (!el) return { found: false, error: `${type}:${value} not found`, selector: '', tag: '', text: '', bbox: {x:0,y:0,width:0,height:0} };

    const r = el.getBoundingClientRect();
    return { found: true, selector: selector(el), tag: el.tagName.toLowerCase(), text: text(el).slice(0, 50), bbox: {x:r.x,y:r.y,width:r.width,height:r.height} };
})
"#;

/// Resolve a live pattern to element info via JS.
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
        // Unprefixed non-numeric defaults to text search
        assert!(matches!(
            Target::parse("Submit"),
            Target::Live(LivePattern::Text(_))
        ));
        assert!(matches!(
            Target::parse("Click Me"),
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

        if let Target::Live(LivePattern::Placeholder(v)) = Target::parse("placeholder:Enter email") {
            assert_eq!(v, "Enter email");
        } else {
            panic!("Expected Placeholder");
        }
    }

    #[test]
    fn as_js_args() {
        assert_eq!(LivePattern::Text("foo".into()).as_js_args(), ("text", "foo"));
        assert_eq!(LivePattern::Placeholder("bar".into()).as_js_args(), ("placeholder", "bar"));
        assert_eq!(LivePattern::Css("div.x".into()).as_js_args(), ("css", "div.x"));
        assert_eq!(LivePattern::Id("myid".into()).as_js_args(), ("id", "myid"));
        assert_eq!(LivePattern::Role("button".into()).as_js_args(), ("role", "button"));
    }
}
