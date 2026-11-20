use gpui::{Bounds, DisplayId, Pixels, point, px, size};

#[allow(unexpected_cfgs)]
pub fn set_dock_icon() {
    use cocoa::appkit::NSApplication;
    use cocoa::base::{id, nil};
    use cocoa::foundation::NSData;
    use objc::{class, msg_send, sel, sel_impl};

    const APP_ICON_PNG: &[u8] = include_bytes!("../../assets/afterlife-icon.png");

    unsafe {
        let ns_app = cocoa::appkit::NSApp();
        let data = NSData::dataWithBytes_length_(
            nil,
            APP_ICON_PNG.as_ptr() as *const std::ffi::c_void,
            APP_ICON_PNG.len() as u64,
        );
        let ns_image: id = msg_send![class!(NSImage), alloc];
        let ns_image: id = msg_send![ns_image, initWithData: data];
        if ns_image != nil {
            ns_app.setApplicationIconImage_(ns_image);
            let _: () = msg_send![ns_image, release];
        }
    }
}

#[allow(unexpected_cfgs)]
pub fn display_visible_bounds(display_id: DisplayId) -> Option<Bounds<Pixels>> {
    use cocoa::appkit::NSScreen;
    use cocoa::base::{id, nil};
    use cocoa::foundation::{NSArray, NSAutoreleasePool, NSDictionary, NSString};
    use objc::{msg_send, sel, sel_impl};

    let target_id = u32::from(display_id);

    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        let screens = NSScreen::screens(nil);
        if screens == nil {
            pool.drain();
            return None;
        }
        let count = NSArray::count(screens);
        let key: id = NSString::alloc(nil)
            .init_str("NSScreenNumber")
            .autorelease();
        let mut found = None;
        for idx in 0..count {
            let screen = NSArray::objectAtIndex(screens, idx);
            let desc = NSScreen::deviceDescription(screen);
            let screen_num = desc.objectForKey_(key);
            if screen_num == nil {
                continue;
            }
            let num: u32 = msg_send![screen_num, unsignedIntValue];
            if num == target_id {
                let frame = NSScreen::frame(screen);
                let visible = NSScreen::visibleFrame(screen);
                let origin_x = (visible.origin.x - frame.origin.x).max(0.0) as f32;
                let origin_y = ((frame.origin.y + frame.size.height)
                    - (visible.origin.y + visible.size.height))
                    .max(0.0) as f32;
                let width = visible.size.width.max(1.0) as f32;
                let height = visible.size.height.max(1.0) as f32;
                found = Some(Bounds::new(
                    point(px(origin_x), px(origin_y)),
                    size(px(width), px(height)),
                ));
                break;
            }
        }
        pool.drain();
        found
    }
}
