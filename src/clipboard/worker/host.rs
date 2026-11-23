//! Clipboard writes on WebKit must stay within the original user gesture.
//! Only recognized desktop Blink/Gecko hosts may add a worker message turn;
//! unknown hosts retain local preparation and the existing rich MIME fallback.
//! https://webkit.org/blog/10855/async-clipboard-api/#security-and-privacy

#[cfg(target_arch = "wasm32")]
pub(super) fn allows_worker() -> bool {
    use wasm_bindgen::JsValue;
    let Some(window) = web_sys::window() else {
        return false;
    };
    let navigator = window.navigator();
    let (Ok(agent), Ok(platform)) = (navigator.user_agent(), navigator.platform()) else {
        return false;
    };
    if !allows_host(&agent, &platform, navigator.max_touch_points()) {
        return false;
    }
    // Missing/older clipboard APIs keep their existing local text fallback.
    let get = |object: &JsValue, name: &str| js_sys::Reflect::get(object, &name.into()).ok();
    let rich_write = get(&navigator, "clipboard")
        .and_then(|clipboard| get(&clipboard, "write"))
        .is_some_and(|write| write.is_function());
    rich_write && get(&window, "ClipboardItem").is_some_and(|class| class.is_function())
}

fn allows_host(agent: &str, platform: &str, touch_points: i32) -> bool {
    if touch_points < 0
        || !agent.starts_with("Mozilla/5.0 ")
        || [
            "iPhone", "iPad", "iPod", "CriOS/", "FxiOS/", "EdgiOS/", "Android", "Mobile/",
        ]
        .iter()
        .any(|token| agent.contains(token))
    {
        return false;
    }
    let desktop = (agent.contains("(Windows NT ") && matches!(platform, "Win32" | "Win64"))
        || (agent.contains("(Macintosh;") && platform == "MacIntel" && touch_points == 0)
        || (agent.contains("(X11;") && platform.starts_with("Linux"));
    if !desktop {
        return false;
    }
    // AppleWebKit alone is not an engine test: desktop Chromium advertises
    // that compatibility token too. iOS Chrome uses CriOS (also in desktop
    // mode), while iPad Firefox may present the unmodified Safari UA.
    // https://chromium.googlesource.com/chromium/src/+/main/docs/ios/user_agent.md
    let product = |prefix: &str| {
        agent
            .split_ascii_whitespace()
            .filter_map(|token| token.strip_prefix(prefix))
            .any(|version| {
                version
                    .split('.')
                    .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            })
    };
    let blink = agent
        .split_ascii_whitespace()
        .any(|token| token == "AppleWebKit/537.36")
        && (product("Chrome/") || product("Chromium/"))
        && agent
            .split_ascii_whitespace()
            .any(|token| token == "Safari/537.36");
    let gecko = !agent.contains("AppleWebKit/")
        && agent
            .split_ascii_whitespace()
            .any(|token| token == "Gecko/20100101")
        && product("Firefox/");
    blink || gecko
}

#[cfg(test)]
mod tests {
    use super::allows_host;

    const CHROME_MAC: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36";
    const SAFARI_MAC: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

    #[test]
    fn desktop_blink_and_gecko_can_prepare_in_worker() {
        for (agent, platform) in [
            (CHROME_MAC, "MacIntel"),
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36 Edg/142.0.0.0",
                "Win32",
            ),
            (
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chromium/142.0.0.0 Safari/537.36",
                "Linux x86_64",
            ),
            (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:145.0) Gecko/20100101 Firefox/145.0",
                "MacIntel",
            ),
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:145.0) Gecko/20100101 Firefox/145.0",
                "Win32",
            ),
            (
                "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0",
                "Linux x86_64",
            ),
        ] {
            assert!(allows_host(agent, platform, 0), "{agent}");
        }
    }

    #[test]
    fn webkit_and_ios_browser_brands_keep_preparation_local() {
        for (agent, platform, touches) in [
            (SAFARI_MAC, "MacIntel", 0),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
                "iPhone",
                5,
            ),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/142.0.7444.59 Mobile/15E148 Safari/604.1",
                "iPhone",
                5,
            ),
            (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/142 Version/11.1.1 Safari/605.1.15",
                "MacIntel",
                5,
            ),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) FxiOS/145.0 Mobile/15E148 Safari/605.1.15",
                "iPhone",
                5,
            ),
            (
                "Mozilla/5.0 (iPad; CPU OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) EdgiOS/142.0 Mobile/15E148 Safari/605.1.15",
                "iPad",
                5,
            ),
            // iPad Firefox's desktop UA can be identical to Safari's.
            (SAFARI_MAC, "MacIntel", 5),
        ] {
            assert!(!allows_host(agent, platform, touches), "{agent}");
        }
    }

    #[test]
    fn ambiguous_hosts_and_runtime_boundaries_keep_preparation_local() {
        for (agent, platform, touches) in [
            ("", "", 0),
            (CHROME_MAC, "", 0),
            (CHROME_MAC, "iPad", 0),
            (CHROME_MAC, "MacIntel", -1),
            (CHROME_MAC, "MacIntel", 1),
            (CHROME_MAC, "MacIntel", 5),
            (
                "Mozilla/5.0 (Windows NT 10.0) AppleWebKit/605.1.15 Chrome/142.0.0.0 Safari/605.1.15",
                "Win32",
                0,
            ),
            (
                "Mozilla/5.0 (Windows NT 10.0) AppleWebKit/537.36 NotChrome/142.0.0.0 Safari/537.36",
                "Win32",
                0,
            ),
            (
                "Mozilla/5.0 (Windows NT 10.0) AppleWebKit/537.36 Chrome/unknown Safari/537.36",
                "Win32",
                0,
            ),
            ("Mozilla/5.0 (Windows NT 10.0) Firefox/145.0", "Win32", 0),
            (
                "Mozilla/5.0 (Linux; Android 15) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Mobile Safari/537.36",
                "Linux armv8l",
                5,
            ),
        ] {
            assert!(
                !allows_host(agent, platform, touches),
                "{agent}, {platform}, {touches}"
            );
        }
    }
}
