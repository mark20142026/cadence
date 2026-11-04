//! The network, the same calls on both builds. `post` sends a form to the game servers, `get`
//! fetches a file (news, songs, sound effects), `spawn` runs a request in the background and
//! hands back a channel for its result.
//!
//! Desktop: the system libcurl (`gjapi::post_raw`, sent exactly like the game's) and ureq, on a
//! thread of their own. Web: `fetch` through Cadence's relay (`relay/worker.js`, served from the
//! same site under `/relay`), which forwards each request unchanged: browsers can't reach the
//! game servers themselves (no CORS headers there).

use std::future::Future;
use std::sync::mpsc::{channel, Receiver};

/// Run `work` in the background; its result arrives on the channel.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn<T: Send + 'static>(work: impl Future<Output = T> + Send + 'static) -> Receiver<T> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(pollster::block_on(work));
    });
    rx
}

#[cfg(target_arch = "wasm32")]
pub fn spawn<T: 'static>(work: impl Future<Output = T> + 'static) -> Receiver<T> {
    let (tx, rx) = channel();
    wasm_bindgen_futures::spawn_local(async move {
        let _ = tx.send(work.await);
    });
    rx
}

/// Run `work` to the end right here (command-line paths on the desktop).
#[cfg(not(target_arch = "wasm32"))]
pub fn block<T>(work: impl Future<Output = T>) -> T {
    pollster::block_on(work)
}

/// POST `body` (already form-encoded) to `endpoint` under the game servers' `/database`.
#[cfg(not(target_arch = "wasm32"))]
pub async fn post(endpoint: &str, body: &str) -> Result<String, String> {
    crate::gjapi::post_raw(endpoint, body)
}

/// POST to a full address: the account server `getAccountURL.php` hands out (cloud saves).
#[cfg(not(target_arch = "wasm32"))]
pub async fn post_url(url: &str, body: &str) -> Result<String, String> {
    crate::gjapi::post_raw_url(url, body)
}

/// GET a file from one of the game's content hosts (or a song's own URL).
#[cfg(not(target_arch = "wasm32"))]
pub async fn get(url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url).call().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

#[cfg(target_arch = "wasm32")]
pub async fn post(endpoint: &str, body: &str) -> Result<String, String> {
    let bytes = fetch("POST", &format!("{}/database/{endpoint}", relay()), Some(body)).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(target_arch = "wasm32")]
pub async fn post_url(url: &str, body: &str) -> Result<String, String> {
    let bytes = fetch("POST", &format!("{}/post?u={}", relay(), crate::gjapi::field(url)), Some(body)).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(target_arch = "wasm32")]
pub async fn get(url: &str) -> Result<Vec<u8>, String> {
    fetch("GET", &format!("{}/get?u={}", relay(), crate::gjapi::field(url)), None).await
}

/// The relay's address: `window.CADENCE_RELAY` if the page sets it, else `relay` beside the page.
#[cfg(target_arch = "wasm32")]
fn relay() -> String {
    web_sys::window()
        .and_then(|w| js_sys::Reflect::get(&w, &"CADENCE_RELAY".into()).ok())
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| "relay".into())
}

#[cfg(target_arch = "wasm32")]
async fn fetch(method: &str, url: &str, body: Option<&str>) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let err = |e: wasm_bindgen::JsValue| format!("{e:?}");
    let init = web_sys::RequestInit::new();
    init.set_method(method);
    if let Some(body) = body {
        init.set_body(&body.into());
        let headers = web_sys::Headers::new().map_err(err)?;
        headers.set("Content-Type", "application/x-www-form-urlencoded").map_err(err)?;
        init.set_headers(&headers);
    }
    let request = web_sys::Request::new_with_str_and_init(url, &init).map_err(err)?;
    let window = web_sys::window().ok_or("no window")?;
    let resp: web_sys::Response = JsFuture::from(window.fetch_with_request(&request)).await.map_err(err)?.dyn_into().map_err(err)?;
    if !resp.ok() {
        return Err(format!("relay: HTTP {}", resp.status()));
    }
    let buf = JsFuture::from(resp.array_buffer().map_err(err)?).await.map_err(err)?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}
