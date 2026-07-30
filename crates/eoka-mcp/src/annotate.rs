use eoka_server::eoka::{Page, Result};

use crate::InteractiveElement;

const ANNOTATE_JS: &str = include_str!("js/annotate.js");

pub async fn annotated_screenshot(page: &Page, elements: &[InteractiveElement]) -> Result<Vec<u8>> {
    if elements.is_empty() {
        return page.screenshot().await;
    }

    let elem_data: Vec<serde_json::Value> = elements
        .iter()
        .map(|el| {
            serde_json::json!({
                "i": el.index,
                "x": el.bbox.x as i32,
                "y": el.bbox.y as i32,
                "w": el.bbox.width as i32,
                "h": el.bbox.height as i32,
            })
        })
        .collect();

    let json = serde_json::to_string(&elem_data).unwrap_or_default();
    let inject_js = ANNOTATE_JS.replace("/*ELEM_DATA*/", &json);

    page.execute(&inject_js).await?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let png = page.screenshot().await?;
    page.execute("document.getElementById('__eoka_overlay')?.remove()")
        .await?;

    Ok(png)
}
