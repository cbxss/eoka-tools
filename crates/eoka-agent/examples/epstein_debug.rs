use eoka::{Browser, StealthConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut config = StealthConfig::default();
    config.headless = false;
    let browser = Browser::launch_with_config(config).await?;
    let page = browser.new_page("https://www.justice.gov/epstein").await?;
    page.wait(5000).await;
    page.click("#age-button-yes").await?;
    page.wait(2000).await;

    // Dump full _source object to see all field names
    let result: String = page.evaluate(r#"
        (async function() {
            let resp = await fetch('/multimedia-search?keys=kompromat&page=0');
            let data = await resp.json();
            let hits = data.hits?.hits || [];
            if (hits.length === 0) return JSON.stringify({error: 'no hits'});
            // Return first hit's full _source plus all field names
            let src = hits[0]._source || {};
            let keys = Object.keys(src);
            // For each key, show type and first 200 chars of value
            let fields = {};
            for (let k of keys) {
                let v = src[k];
                let t = typeof v;
                if (t === 'string') {
                    fields[k] = {type: t, length: v.length, preview: v.substring(0, 200)};
                } else {
                    fields[k] = {type: t, value: v};
                }
            }
            return JSON.stringify({
                total: data.hits?.total?.value,
                field_count: keys.length,
                field_names: keys,
                fields: fields,
                highlight: hits[0].highlight || null
            });
        })()
    "#).await?;

    let parsed: serde_json::Value = serde_json::from_str(&result)?;
    println!("{}", serde_json::to_string_pretty(&parsed)?);

    browser.close().await?;
    Ok(())
}
