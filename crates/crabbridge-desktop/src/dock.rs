//! macOS Dock icon helpers.

#[cfg(target_os = "macos")]
mod imp {
    use anyhow::{Context, Result};
    use objc2::{AllocAnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    const DOCK_ICON_BYTES: &[u8] = include_bytes!("../icons/icon.png");

    pub fn apply_dock_icon() -> Result<()> {
        let mtm = MainThreadMarker::new().context("dock icon must be set on the main thread")?;
        let app = NSApplication::sharedApplication(mtm);
        let data = NSData::with_bytes(DOCK_ICON_BYTES);
        let image = NSImage::initWithData(NSImage::alloc(), &data)
            .context("failed to decode CrabBridge dock icon")?;
        unsafe {
            app.setApplicationIconImage(Some(&image));
        }
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use anyhow::Result;

    pub fn apply_dock_icon() -> Result<()> {
        Ok(())
    }
}

pub use imp::apply_dock_icon;
