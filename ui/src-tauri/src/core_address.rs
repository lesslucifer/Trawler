// Before first paint is the whole point. Telling the page later means navigating
// it, and a navigation is what the shell cannot survive here: WKWebView stops
// the custom-scheme tasks the Tauri IPC rides on, and wry answers a stopped task
// with an Objective-C exception across a nounwind boundary, which aborts the
// process.
pub fn init<R: tauri::Runtime>(address: &str) -> tauri::plugin::TauriPlugin<R> {
    let builder = tauri::plugin::Builder::new("trawler-core-address");
    match core_address_script(Some(address)) {
        Some(script) => builder.js_init_script(script),
        None => builder,
    }
    .build()
}

// The same global `api.ts` resolves first, written as JSON so an address can
// never leave the string it arrives as.
fn core_address_script(address: Option<&str>) -> Option<String> {
    let address = address.map(str::trim).filter(|a| !a.is_empty())?;
    Some(format!("window.__TRAWLER_API__ = {};", serde_json::Value::from(address)))
}

#[cfg(test)]
mod tests {
    use super::core_address_script;

    #[test]
    fn an_address_becomes_the_global_the_frontend_reads_first() {
        assert_eq!(
            core_address_script(Some("http://127.0.0.1:63357")).unwrap(),
            r#"window.__TRAWLER_API__ = "http://127.0.0.1:63357";"#
        );
    }

    // A blank address would pin every webview to nowhere; the shipped default
    // is the better guess.
    #[test]
    fn an_absent_or_blank_address_injects_nothing() {
        assert!(core_address_script(None).is_none());
        assert!(core_address_script(Some("   ")).is_none());
    }

    // The address crosses a process boundary as text, so it is untrusted input to
    // the script it lands in: every quote it carries stays inside the literal.
    #[test]
    fn a_quote_in_the_address_cannot_close_the_string_it_sits_in() {
        assert_eq!(
            core_address_script(Some(r#"http://x";window.pwned=1;""#)).unwrap(),
            r#"window.__TRAWLER_API__ = "http://x\";window.pwned=1;\"";"#
        );
    }
}
