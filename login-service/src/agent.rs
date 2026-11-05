#![allow(unsafe_code)]

use objc2::rc::Retained;
use objc2_foundation::NSString;
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

impl AgentService {
    pub fn new(plist_name: &str) -> Self {
        Self { plist_name: plist_name.to_owned() }
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
