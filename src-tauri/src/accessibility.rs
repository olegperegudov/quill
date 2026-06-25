//! macOS Accessibility (AX) trust — may Quill post synthetic keystrokes?
//!
//! Reading the selection (synthetic ⌘C) and typing the result back both need
//! the app to be trusted for Accessibility. This is the permission that breaks
//! on every release: an ad-hoc-signed build gets a fresh cdhash, the old grant
//! no longer matches, and posting events just *silently fails*. `tcc_reset.rs`
//! clears the stale grant; this module is what actually asks for it back — in
//! the user's face, via the real macOS dialog — instead of failing quietly.
//!
//! Off-macOS there is nothing to grant, so both calls report "trusted".

#[cfg(target_os = "macos")]
mod imp {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::{CFString, CFStringRef};

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
    }

    /// Are we already trusted? No dialog, no side effects — safe to poll.
    pub fn is_trusted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    /// Like `is_trusted`, but when we're *not* trusted it asks macOS to show the
    /// system "allow Quill to control this computer" dialog (with an Open System
    /// Settings button). Returns the trust status at call time.
    pub fn prompt() -> bool {
        unsafe {
            let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
            let opts = CFDictionary::from_CFType_pairs(&[(
                key.as_CFType(),
                CFBoolean::true_value().as_CFType(),
            )]);
            AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef())
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn is_trusted() -> bool { true }
    pub fn prompt() -> bool { true }
}

pub use imp::{is_trusted, prompt};
