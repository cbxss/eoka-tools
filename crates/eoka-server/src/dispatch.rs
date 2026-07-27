use serde_json::Value;

use crate::methods::{browser, page};
use crate::protocol::ServerError;
use crate::state::AppState;

pub async fn dispatch(
    state: &mut AppState,
    method: &str,
    params: Value,
) -> Result<Value, ServerError> {
    match method {
        "browser.launch" => browser::launch(state, params).await,
        "browser.new_page" => browser::new_page(state, params).await,
        "browser.tabs" => browser::tabs(state, params).await,
        "browser.close_tab" => browser::close_tab(state, params).await,
        "browser.close" => browser::close(state, params).await,

        "page.goto" => page::goto(state, params).await,
        "page.click" => page::click(state, params).await,
        "page.click_text" => page::click_text(state, params).await,
        "page.human_click" => page::human_click(state, params).await,
        "page.human_click_text" => page::human_click_text(state, params).await,
        "page.fill" => page::fill(state, params).await,
        "page.human_fill" => page::human_fill(state, params).await,
        "page.type_into" => page::type_into(state, params).await,
        "page.text" => page::text(state, params).await,
        "page.content" => page::content(state, params).await,
        "page.title" => page::title(state, params).await,
        "page.url" => page::url(state, params).await,
        "page.get_text" => page::get_text(state, params).await,
        "page.get_attribute" => page::get_attribute(state, params).await,
        "page.exists" => page::exists(state, params).await,
        "page.wait_for" => page::wait_for(state, params).await,
        "page.wait_for_visible" => page::wait_for_visible(state, params).await,
        "page.wait_for_text" => page::wait_for_text(state, params).await,
        "page.evaluate" => page::evaluate(state, params).await,
        "page.execute" => page::execute(state, params).await,
        "page.fetch" => page::fetch(state, params).await,
        "page.capture_state" => page::capture_state(state, params).await,
        "page.restore_state" => page::restore_state(state, params).await,
        "page.screenshot" => page::screenshot(state, params).await,
        "page.select" => page::select(state, params).await,
        "page.hover" => page::hover(state, params).await,
        "page.press_key" => page::press_key(state, params).await,
        "page.solve_captcha" => page::solve_captcha(state, params).await,
        "page.close" => page::close(state, params).await,

        _ => Err(ServerError::unknown_method(method)),
    }
}
