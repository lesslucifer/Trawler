// The ask that arming reach costs (DEC-55). It lives here for the same reason
// the CA install and the system-proxy write do (DEC-13): the shell is where
// Trawler asks before changing what this machine exposes. The bind itself is the
// core's — this command only carries the question and the answer.

use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

const TITLE: &str = "Let other devices reach Trawler?";
const ALLOW: &str = "Allow";
const CANCEL: &str = "Cancel";

// Names the address, and names what becomes reachable through it — the proxy is
// the part a user asked for, the captured flows are the part they did not and
// must still be told about.
fn confirm_message(address: &str) -> String {
    format!(
        "Trawler will listen on {address} so another device can route through it.\n\n\
         Anything on that network will be able to send traffic through the proxy and to read \
         and clear the flows Trawler has captured. Trawler stops listening there when you turn \
         this off, and whenever it quits."
    )
}

#[tauri::command]
pub async fn confirm_reach(app: tauri::AppHandle, address: String) -> Result<bool, String> {
    let (answered, answer) = std::sync::mpsc::channel();
    app.dialog()
        .message(confirm_message(&address))
        .title(TITLE)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(ALLOW.into(), CANCEL.into()))
        .show(move |allowed| {
            let _ = answered.send(allowed);
        });
    // The dialog answers on the main thread, so the wait cannot happen on it.
    tauri::async_runtime::spawn_blocking(move || answer.recv().unwrap_or(false))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::confirm_message;

    #[test]
    fn the_ask_names_the_address_it_is_about() {
        assert!(confirm_message("192.168.1.20").contains("192.168.1.20"));
    }

    // A confirm that said only "let devices use the proxy" would be asking for
    // consent to something narrower than what it grants.
    #[test]
    fn the_ask_states_the_capture_becomes_readable_not_just_the_proxy_usable() {
        let message = confirm_message("192.168.1.20");
        assert!(message.contains("proxy"), "{message}");
        assert!(message.contains("read"), "{message}");
        assert!(message.contains("flows Trawler has captured"), "{message}");
    }

    #[test]
    fn the_ask_says_the_exposure_ends_with_the_session() {
        assert!(confirm_message("10.0.0.5").contains("whenever it quits"));
    }
}
