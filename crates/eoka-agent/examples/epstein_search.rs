//! Search DOJ Epstein Library via in-browser fetch().
//! API returns Elasticsearch JSON. Browser handles Akamai.

use eoka::{Browser, StealthConfig};
use std::io::Write;
use std::time::Instant;

const SEARCH_TERMS: &[&str] = &[
    // Epstein's known emails
    "jeevacation@gmail.com",
    "jeeitunes@gmail.com",
    // Key associates by email domain/pattern
    "@gmail.com Jeffrey",
    "@yahoo.com Jeffrey",
    "@aol.com",
    "@hotmail.com",
    "@icloud.com",
    "@me.com",
    "@mac.com",
    "@outlook.com",
    "@protonmail.com",
    // Known associates - search by name + email context
    "Ghislaine Maxwell email",
    "Lesley Groff email",
    "Sarah Kellen email",
    "Nadia Marcinkova email",
    "Adriana Ross email",
    "Richard Kahn email",
    "Darren Indyke email",
    "Bella Klein email",
    // Known names to find their emails
    "From: Brock Pierce",
    "From: Steve Bannon",
    "From: Deepak Chopra",
    "From: Joi Ito",
    "From: Karyna Shuliak",
    "From: Leon Black",
    "From: Les Wexner",
    "From: Reid Hoffman",
    "From: Bill Gates",
    "From: Ehud Barak",
    "From: Alan Dershowitz",
    "From: Larry Summers",
    "From: Woody Allen",
    "From: Noam Chomsky",
    "From: Eva Andersson",
    "From: Jean-Luc Brunel",
    "From: Sheldon Adelson",
    "From: Prince Andrew",
    "From: Naomi Campbell",
    "From: Steven Pinker",
    "From: Marvin Minsky",
    "From: Ben Goertzel",
    "From: Joshua Cooper Ramo",
    "From: Al Seckel",
    "From: Steven Sinofsky",
    "From: Bobby Kotick",
    "From: Faith Kates",
    "From: Pablos Holman",
    "From: Terry Kafka",
    "From: Masha Drokova",
    "From: Vincenzo Iozzo",
    "From: Dan Fleuette",
    "From: Sean Kelly",
    "From: Lawrence Krauss",
    "From: Martin Nowak",
    "From: Danny Hillis",
    "From: Peter Thiel",
    "From: Elon Musk",
    "From: Mark Zuckerberg",
    // Email patterns from known domains
    "@hbrk",
    "@elliptic",
    "@lfrk.law",
    "arinaballerina",
    "renbolotova",
    // More known contacts
    "From: Celina Dubin",
    "From: Glenn Dubin",
    "From: Michael Wolff",
    "From: Henry Jarecki",
    "From: Mort Zuckerman",
    "From: Edgar Bronfman",
    "From: Lynn Forester",
    "From: Katie Couric",
    "From: George Stephanopoulos",
    "From: Sergey Brin",
    "From: Jeff Bezos",
    "From: Jes Staley",
    "From: Jamie Dimon",
    "From: Marie-Joseph Experton",
    "From: Daphne Wallace",
    "From: Carlos Rodriguez",
    "From: Ike Groff",
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let start = Instant::now();

    let mut config = StealthConfig::default();
    config.headless = false;
    let browser = Browser::launch_with_config(config).await?;
    let page = browser.new_page("https://www.justice.gov/epstein").await?;
    page.wait(5000).await;
    page.click("#age-button-yes").await?;
    page.wait(2000).await;

    let mut out = std::fs::File::create("/tmp/epstein_emails.txt")?;

    let header = format!("=== DOJ Epstein Library - Email Search ===\n=== {} terms ===\n", SEARCH_TERMS.len());
    print!("{}", header);
    write!(out, "{}", header)?;

    for (i, term) in SEARCH_TERMS.iter().enumerate() {
        eprint!("[{}/{}] \"{}\"... ", i + 1, SEARCH_TERMS.len(), term);

        let escaped = term.replace('\\', "\\\\").replace('\'', "\\'");
        let js = format!(r#"
            (async function() {{
                try {{
                    let resp = await fetch('/multimedia-search?keys={}&page=0');
                    if (!resp.ok) return JSON.stringify({{ error: resp.status }});
                    let data = await resp.json();

                    let total = data.hits?.total?.value || 0;
                    let hits = data.hits?.hits || [];

                    let results = hits.map(h => {{
                        let s = h._source || {{}};
                        let hl = h.highlight?.content || [];
                        let snippet = hl.join(' ... ').replace(/<\/?em>/g, '*');
                        return {{
                            file: s.ORIGIN_FILE_NAME || '?',
                            uri: s.ORIGIN_FILE_URI || '',
                            pages: (s.startPage || '') + '-' + (s.endPage || ''),
                            snippet: snippet.substring(0, 400)
                        }};
                    }});

                    return JSON.stringify({{ total: total, results: results }});
                }} catch(e) {{
                    return JSON.stringify({{ error: e.message }});
                }}
            }})()
        "#, urlencoding::encode(&escaped));

        match page.evaluate::<String>(&js).await {
            Ok(raw) => {
                let parsed: serde_json::Value = serde_json::from_str(&raw)?;

                if let Some(err) = parsed["error"].as_str() {
                    eprintln!("ERROR: {}", err);
                    continue;
                }

                let total = parsed["total"].as_u64().unwrap_or(0);
                eprintln!("{} results", total);

                if total > 0 {
                    let line = format!("--- \"{}\" --- {} results ---\n", term, total);
                    print!("{}", line);
                    write!(out, "{}", line)?;
                    if let Some(results) = parsed["results"].as_array() {
                        for r in results.iter().take(10) {
                            let file = r["file"].as_str().unwrap_or("?");
                            let pages = r["pages"].as_str().unwrap_or("");
                            let snippet = r["snippet"].as_str().unwrap_or("")
                                .replace('\n', " ");
                            let snippet: String = snippet.chars().take(300).collect();
                            let line = format!("  {} [p{}] {}\n", file, pages, snippet.trim());
                            print!("{}", line);
                            write!(out, "{}", line)?;
                        }
                    }
                    println!();
                    writeln!(out)?;
                }
            }
            Err(e) => eprintln!("EVAL ERROR: {}", e),
        }

        page.wait(150).await;
    }

    let done = format!("\n=== Done in {:.1}s ===\n", start.elapsed().as_secs_f64());
    print!("{}", done);
    write!(out, "{}", done)?;
    eprintln!("Results saved to /tmp/epstein_emails.txt");
    browser.close().await?;
    Ok(())
}
