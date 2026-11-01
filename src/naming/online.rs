//! Optional PubChem database lookup. Call only after consent for this exact name.
use super::{Cancel, Error, Provenance, Record, canonical_smiles, local};
use serde::Deserialize;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use std::{future::Future, time::Duration};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

const API: &str = "https://pubchem.ncbi.nlm.nih.gov/rest/pug/compound";
const LIMIT: usize = 65_536;
const DEADLINE: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(25);
// Serialize lookups and pause before each request: at most four requests/sec,
// below PubChem's five/sec limit, including transitions between lookups.
static SLOT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

/// Resolve one complete-name database match, retaining its full returned graph.
/// No network request is made by local naming or before the caller's consent.
/// Cancellation, timeout, and dropping this future abort its active request.
pub async fn resolve_name_online(name: &str, cancel: Cancel) -> Result<Record, Error> {
    if cancel.stopped() {
        return Err(cancelled());
    }
    let name = local::checked_name(name)?;
    if matches!(name, "." | "..") {
        return Err(Error::Rejected("Enter a complete chemical name".into()));
    }
    bounded(cancel, DEADLINE, || async {
        let _permit = SLOT.acquire().await.map_err(|_| unavailable())?;
        let transport = Transport::new()?;
        pause(Duration::from_millis(250)).await?;
        let cid = parse_cid(&transport.get(&name_url(name)).await?)?;
        pause(Duration::from_millis(250)).await?;
        let url = format!("{API}/cid/{cid}/property/SMILES,IUPACName/JSON");
        parse_record(name, cid, &transport.get(&url).await?)
    })
    .await
}

fn name_url(name: &str) -> String {
    // Encode a UTF-8 path segment, including '/', '+', '#' and '%'.
    let mut encoded = String::new();
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    format!("{API}/name/{encoded}/cids/JSON?name_type=complete")
}

fn unavailable() -> Error {
    Error::Worker(
        "PubChem could not complete this lookup. Check the connection or try again later.".into(),
    )
}
fn cancelled() -> Error {
    Error::Cancelled("Online name lookup was cancelled".into())
}
fn invalid() -> Error {
    Error::Worker("PubChem returned an invalid or incomplete molecular response".into())
}
fn too_large() -> Error {
    Error::Worker("PubChem's response exceeded the 64 KiB lookup limit".into())
}

struct StopOnDrop(Cancel);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.stop();
    }
}

async fn bounded<T, F, Fut>(cancel: Cancel, deadline: Duration, request: F) -> Result<T, Error>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, Error>>,
{
    if cancel.stopped() {
        return Err(cancelled());
    }
    let _stop = StopOnDrop(cancel.clone());
    let request = std::pin::pin!(request());
    let watchdog = std::pin::pin!(async {
        let started = Instant::now();
        loop {
            if cancel.stopped() {
                return Err(cancelled());
            }
            if started.elapsed() >= deadline {
                return Err(Error::Worker(
                    "PubChem lookup reached its 15-second time limit".into(),
                ));
            }
            pause(POLL).await?;
        }
    });
    match iced::futures::future::select(request, watchdog).await {
        iced::futures::future::Either::Left((result, _)) => {
            if cancel.stopped() {
                Err(cancelled())
            } else {
                result
            }
        }
        iced::futures::future::Either::Right((result, _)) => result,
    }
}

#[derive(Deserialize)]
struct Identifiers {
    #[serde(rename = "IdentifierList")]
    list: Cids,
}
#[derive(Deserialize)]
struct Cids {
    #[serde(rename = "CID")]
    ids: Vec<u64>,
}
fn parse_cid(bytes: &[u8]) -> Result<u64, Error> {
    if bytes.len() > LIMIT {
        return Err(too_large());
    }
    let reply: Identifiers = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let Some(&cid) = reply.list.ids.first() else {
        return Err(Error::Rejected(
            "PubChem found no complete-name match".into(),
        ));
    };
    if cid == 0 || reply.list.ids.contains(&0) {
        return Err(invalid());
    }
    if reply.list.ids.iter().any(|&other| other != cid) {
        return Err(Error::Rejected("This name matches multiple PubChem structures. Enter a more specific name; no structure was selected.".into()));
    }
    Ok(cid)
}

#[derive(Deserialize)]
struct Properties {
    #[serde(rename = "PropertyTable")]
    table: Table,
}
#[derive(Deserialize)]
struct Table {
    #[serde(rename = "Properties")]
    properties: Vec<Property>,
}
#[derive(Deserialize)]
struct Property {
    #[serde(rename = "CID")]
    cid: u64,
    #[serde(rename = "SMILES")]
    smiles: String,
    #[serde(rename = "IUPACName")]
    name: Option<String>,
}
fn parse_record(name: &str, cid: u64, bytes: &[u8]) -> Result<Record, Error> {
    if bytes.len() > LIMIT {
        return Err(too_large());
    }
    let reply: Properties = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let [property] = reply.table.properties.as_slice() else {
        return Err(Error::Rejected(
            "PubChem did not return exactly one molecular structure; no structure was selected."
                .into(),
        ));
    };
    if property.cid != cid || cid == 0 {
        return Err(invalid());
    }
    if property.smiles.is_empty() || property.smiles.chars().any(char::is_whitespace) {
        return Err(invalid());
    }
    if property.smiles.contains('.') {
        return Err(Error::Disconnected("PubChem returned a disconnected salt or mixture. No component was removed or substituted; import a single connected molecule instead.".into()));
    }
    let canonical = canonical_smiles(&property.smiles).map_err(Error::Rejected)?;
    if canonical.contains('.') {
        return Err(invalid());
    }
    let systematic_name = property
        .name
        .as_deref()
        .map(local::checked_name)
        .transpose()
        .map_err(|_| invalid())?
        .map(str::to_owned);
    Ok(Record {
        title: name.into(),
        systematic_name,
        name_status: None,
        smiles: property.smiles.clone(),
        canonical_smiles: canonical,
        synonyms: vec![],
        warnings: vec!["A PubChem database match does not prove that the name has only one possible meaning. Review the complete structure and its specified stereochemistry before inserting.".into()],
        provenance: Provenance::PubChem { cid },
    })
}

fn checked_response(status: u16, bytes: Vec<u8>) -> Result<Vec<u8>, Error> {
    if status == 200 {
        return Ok(bytes);
    }
    if status == 404 {
        let fault: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
        if fault
            .pointer("/Fault/Code")
            .and_then(serde_json::Value::as_str)
            == Some("PUGREST.NotFound")
        {
            return Err(Error::Rejected(
                "PubChem found no complete-name match".into(),
            ));
        }
    }
    if matches!(status, 429 | 503) {
        return Err(Error::Worker(
            "PubChem is busy or limiting requests. Try this lookup again later.".into(),
        ));
    }
    Err(Error::Worker(format!(
        "PubChem lookup failed (HTTP {status}); no structure was accepted"
    )))
}

#[cfg(not(target_arch = "wasm32"))]
struct Transport(reqwest::Client);
#[cfg(not(target_arch = "wasm32"))]
impl Transport {
    fn new() -> Result<Self, Error> {
        reqwest::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(DEADLINE)
            .user_agent(concat!("ReShiki/", env!("CARGO_PKG_VERSION")))
            .build()
            .map(Self)
            .map_err(|_| unavailable())
    }
    async fn get(&self, url: &str) -> Result<Vec<u8>, Error> {
        let mut response = self
            .0
            .get(url)
            .header("Accept", "application/json")
            .header("Accept-Encoding", "identity")
            .send()
            .await
            .map_err(|_| unavailable())?;
        if response
            .content_length()
            .is_some_and(|length| length > LIMIT as u64)
        {
            return Err(too_large());
        }
        let status = response.status().as_u16();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
            if chunk.len() > LIMIT - bytes.len() {
                return Err(too_large());
            }
            bytes.extend_from_slice(&chunk);
        }
        checked_response(status, bytes)
    }
}

#[cfg(target_arch = "wasm32")]
struct Transport;
#[cfg(target_arch = "wasm32")]
impl Transport {
    fn new() -> Result<Self, Error> {
        Ok(Self)
    }
    async fn get(&self, url: &str) -> Result<Vec<u8>, Error> {
        use wasm_bindgen::{JsCast, JsValue};
        use wasm_bindgen_futures::JsFuture;
        struct Abort(web_sys::AbortController);
        impl Drop for Abort {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let abort = Abort(web_sys::AbortController::new().map_err(|_| unavailable())?);
        let init = web_sys::RequestInit::new();
        init.set_method("GET");
        init.set_redirect(web_sys::RequestRedirect::Error);
        init.set_credentials(web_sys::RequestCredentials::Omit);
        init.set_referrer_policy(web_sys::ReferrerPolicy::NoReferrer);
        init.set_cache(web_sys::RequestCache::NoStore);
        init.set_signal(Some(&abort.0.signal()));
        let request =
            web_sys::Request::new_with_str_and_init(url, &init).map_err(|_| unavailable())?;
        request
            .headers()
            .set("Accept", "application/json")
            .map_err(|_| unavailable())?;
        let window = web_sys::window().ok_or_else(unavailable)?;
        let response = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|_| unavailable())?
            .dyn_into::<web_sys::Response>()
            .map_err(|_| invalid())?;
        if response.redirected() || response.url() != request.url() {
            return Err(invalid());
        }
        if response
            .headers()
            .get("Content-Length")
            .map_err(|_| invalid())?
            .and_then(|s| s.parse::<u64>().ok())
            .is_some_and(|length| length > LIMIT as u64)
        {
            return Err(too_large());
        }
        let status = response.status();
        let reader = response
            .body()
            .ok_or_else(invalid)?
            .get_reader()
            .dyn_into::<web_sys::ReadableStreamDefaultReader>()
            .map_err(|_| invalid())?;
        let mut bytes = Vec::new();
        loop {
            let item = JsFuture::from(reader.read())
                .await
                .map_err(|_| unavailable())?;
            if js_sys::Reflect::get(&item, &JsValue::from_str("done"))
                .map_err(|_| invalid())?
                .as_bool()
                == Some(true)
            {
                break;
            }
            let chunk = js_sys::Reflect::get(&item, &JsValue::from_str("value"))
                .map_err(|_| invalid())?
                .dyn_into::<js_sys::Uint8Array>()
                .map_err(|_| invalid())?;
            let length = chunk.length() as usize;
            if length > LIMIT - bytes.len() {
                return Err(too_large());
            }
            let start = bytes.len();
            bytes.resize(start + length, 0);
            let destination = bytes.get_mut(start..).ok_or_else(invalid)?;
            chunk.copy_to(destination);
        }
        checked_response(status, bytes)
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn pause(duration: Duration) -> Result<(), Error> {
    tokio::time::sleep(duration).await;
    Ok(())
}
#[cfg(target_arch = "wasm32")]
async fn pause(duration: Duration) -> Result<(), Error> {
    let window = web_sys::window().ok_or_else(unavailable)?;
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        if let Err(error) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            &resolve,
            duration.as_millis() as i32,
        ) {
            let _ = reject.call1(&wasm_bindgen::JsValue::NULL, &error);
        }
    });
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map(|_| ())
        .map_err(|_| unavailable())
}

#[cfg(test)]
mod tests;
