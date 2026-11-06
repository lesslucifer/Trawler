#![allow(unsafe_code)]

use objc2::rc::Retained;
use objc2_foundation::{NSOperatingSystemVersion, NSProcessInfo, NSString};
use objc2_service_management::{SMAppService, SMAppServiceStatus};

pub struct AgentService {
    plist_name: String,
}

fn is_enabled(status: SMAppServiceStatus) -> bool {
    status == SMAppServiceStatus::Enabled
}

fn is_registered(status: SMAppServiceStatus) -> bool {
    status != SMAppServiceStatus::NotRegistered && status != SMAppServiceStatus::NotFound
}

const SMAPPSERVICE_SINCE: NSOperatingSystemVersion =
    NSOperatingSystemVersion { majorVersion: 13, minorVersion: 0, patchVersion: 0 };

impl AgentService {
    // Before macOS 13 the class does not exist and objc2 panics at its first use,
    // so no service is handed out where it could be reached.
    pub fn new(plist_name: &str) -> Option<Self> {
        Self::on_hosts_since(plist_name, SMAPPSERVICE_SINCE)
    }

    fn on_hosts_since(plist_name: &str, since: NSOperatingSystemVersion) -> Option<Self> {
        NSProcessInfo::processInfo()
            .isOperatingSystemAtLeastVersion(since)
            .then(|| Self { plist_name: plist_name.to_owned() })
    }

    fn service(&self) -> Retained<SMAppService> {
        unsafe { SMAppService::agentServiceWithPlistName(&NSString::from_str(&self.plist_name)) }
    }

    pub fn enabled(&self) -> bool {
        is_enabled(unsafe { self.service().status() })
    }

    pub fn register(&self) -> Result<(), String> {
        unsafe { self.service().registerAndReturnError() }
            .map_err(|err| err.localizedDescription().to_string())
    }

    pub fn unregister(&self) -> Result<(), String> {
        let service = self.service();
        if !is_registered(unsafe { service.status() }) {
            return Ok(());
        }
        unsafe { service.unregisterAndReturnError() }
            .map_err(|err| err.localizedDescription().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(major: isize) -> NSOperatingSystemVersion {
        NSOperatingSystemVersion { majorVersion: major, minorVersion: 0, patchVersion: 0 }
    }

    #[test]
    fn the_service_is_handed_out_only_from_macos_13() {
        assert_eq!(SMAPPSERVICE_SINCE, version(13));
        assert!(AgentService::new("dev.trawler.app.plist").is_some(), "this host runs macOS 13+");
    }

    #[test]
    fn a_host_older_than_the_class_gets_no_service() {
        assert!(
            AgentService::on_hosts_since("dev.trawler.app.plist", version(isize::MAX)).is_none()
        );
    }

    #[test]
    fn only_enabled_reads_as_on() {
        assert!(is_enabled(SMAppServiceStatus::Enabled));
        for status in [
            SMAppServiceStatus::RequiresApproval,
            SMAppServiceStatus::NotRegistered,
            SMAppServiceStatus::NotFound,
        ] {
            assert!(!is_enabled(status));
        }
    }

    #[test]
    fn only_a_known_service_is_unregistered() {
        assert!(!is_registered(SMAppServiceStatus::NotRegistered));
        assert!(!is_registered(SMAppServiceStatus::NotFound));
        assert!(is_registered(SMAppServiceStatus::Enabled));
        assert!(is_registered(SMAppServiceStatus::RequiresApproval));
    }
}
