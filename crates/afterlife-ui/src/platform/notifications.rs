use std::sync::{Mutex, OnceLock};

use tokio::sync::mpsc::UnboundedSender;

use crate::app::notifications::ApprovalNotificationTarget;

static CLICK_TX: OnceLock<Mutex<Option<UnboundedSender<String>>>> = OnceLock::new();

fn click_slot() -> &'static Mutex<Option<UnboundedSender<String>>> {
    CLICK_TX.get_or_init(|| Mutex::new(None))
}

#[cfg(target_os = "macos")]
#[link(name = "UserNotifications", kind = "framework")]
unsafe extern "C" {}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
pub fn has_macos_app_bundle() -> bool {
    use cocoa::base::{id, nil};
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let bundle: id = msg_send![class!(NSBundle), mainBundle];
        if bundle == nil {
            return false;
        }
        let ident: id = msg_send![bundle, bundleIdentifier];
        if ident == nil {
            return false;
        }
        let path: id = msg_send![bundle, bundlePath];
        if path == nil {
            return false;
        }
        let cstr: *const std::os::raw::c_char = msg_send![path, UTF8String];
        if cstr.is_null() {
            return false;
        }
        std::ffi::CStr::from_ptr(cstr)
            .to_string_lossy()
            .ends_with(".app")
    }
}

#[cfg(not(target_os = "macos"))]
pub fn has_macos_app_bundle() -> bool {
    false
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
mod macos_impl {
    use std::sync::Once;

    use cocoa::{
        base::{BOOL, YES, id, nil},
        foundation::{NSAutoreleasePool, NSString},
    };
    use objc::{
        class,
        declare::ClassDecl,
        msg_send,
        runtime::{Class, Object, Sel},
        sel, sel_impl,
    };

    use super::{click_slot, has_macos_app_bundle};
    use crate::app::notifications::ApprovalNotificationTarget;

    static REGISTER_DELEGATE: Once = Once::new();
    static mut DELEGATE_INSTANCE: id = nil;

    extern "C" fn did_receive_response(
        _this: &Object,
        _cmd: Sel,
        _center: id,
        response: id,
        completion_handler: id,
    ) {
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let notification: id = msg_send![response, notification];
            let request: id = msg_send![notification, request];
            let content: id = msg_send![request, content];
            let user_info: id = msg_send![content, userInfo];
            if user_info != nil {
                let key = NSString::alloc(nil).init_str("job_id").autorelease();
                let val: id = msg_send![user_info, objectForKey: key];
                if val != nil {
                    let cstr: *const std::os::raw::c_char = msg_send![val, UTF8String];
                    if !cstr.is_null() {
                        let job_id = std::ffi::CStr::from_ptr(cstr)
                            .to_string_lossy()
                            .into_owned();
                        if let Ok(guard) = click_slot().lock()
                            && let Some(ref tx) = *guard
                        {
                            let _ = tx.send(job_id);
                        }
                    }
                }
            }
            let ns_app = cocoa::appkit::NSApp();
            if ns_app != nil {
                let _: () = msg_send![ns_app, activateIgnoringOtherApps: YES];
            }
            if completion_handler != nil {
                let blk = completion_handler as *mut block::Block<(), ()>;
                (*blk).call(());
            }
            pool.drain();
        }
    }

    fn ensure_delegate_instance() -> id {
        REGISTER_DELEGATE.call_once(|| unsafe {
            let superclass = Class::get("NSObject").expect("NSObject class missing");
            let mut decl = ClassDecl::new("AfterlifeNotificationDelegate", superclass)
                .expect("declare AfterlifeNotificationDelegate");
            decl.add_method(
                sel!(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:),
                did_receive_response as extern "C" fn(&Object, Sel, id, id, id),
            );
            let cls = decl.register();
            let obj: id = msg_send![cls, alloc];
            DELEGATE_INSTANCE = msg_send![obj, init];
        });
        unsafe { DELEGATE_INSTANCE }
    }

    pub fn init() {
        if !has_macos_app_bundle() {
            return;
        }
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let center: id = msg_send![class!(UNUserNotificationCenter), currentNotificationCenter];
            if center != nil {
                let delegate = ensure_delegate_instance();
                let _: () = msg_send![center, setDelegate: delegate];
                let auth_block = block::ConcreteBlock::new(|_granted: BOOL, _err: id| {}).copy();
                let options: u64 = (1 << 0) | (1 << 2);
                let _: () = msg_send![
                    center,
                    requestAuthorizationWithOptions: options
                    completionHandler: &*auth_block
                ];
            }
            pool.drain();
        }
    }

    pub fn deliver(target: &ApprovalNotificationTarget) {
        if !has_macos_app_bundle() {
            return;
        }
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let ns_app = cocoa::appkit::NSApp();
            if ns_app == nil {
                pool.drain();
                return;
            }
            let is_active: BOOL = msg_send![ns_app, isActive];
            if is_active == YES {
                pool.drain();
                return;
            }
            let center: id = msg_send![class!(UNUserNotificationCenter), currentNotificationCenter];
            if center == nil {
                pool.drain();
                return;
            }
            let content: id = msg_send![class!(UNMutableNotificationContent), alloc];
            let content: id = msg_send![content, init];
            let title = NSString::alloc(nil)
                .init_str("Approval Needed")
                .autorelease();
            let subtitle_text = format!("{} · {}", target.job_id, target.district_name);
            let subtitle = NSString::alloc(nil)
                .init_str(&subtitle_text)
                .autorelease();
            let body = NSString::alloc(nil)
                .init_str(&target.job_title)
                .autorelease();
            let _: () = msg_send![content, setTitle: title];
            let _: () = msg_send![content, setSubtitle: subtitle];
            let _: () = msg_send![content, setBody: body];
            let _: () = msg_send![content, setSound: nil];

            let key = NSString::alloc(nil).init_str("job_id").autorelease();
            let val = NSString::alloc(nil)
                .init_str(target.job_id.as_str())
                .autorelease();
            let user_info: id = msg_send![
                class!(NSDictionary),
                dictionaryWithObject: val
                forKey: key
            ];
            let _: () = msg_send![content, setUserInfo: user_info];

            let req_id_text = format!("afterlife-approval-{}-{}", target.job_id, target.entry_key);
            let req_id = NSString::alloc(nil).init_str(&req_id_text).autorelease();
            let request: id = msg_send![
                class!(UNNotificationRequest),
                requestWithIdentifier: req_id
                content: content
                trigger: nil
            ];
            let _: () = msg_send![center, addNotificationRequest: request withCompletionHandler: nil];
            let _: () = msg_send![content, release];
            pool.drain();
        }
    }

    pub fn sync_badge(approval_count: usize, enabled: bool) {
        if !has_macos_app_bundle() {
            return;
        }
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let ns_app = cocoa::appkit::NSApp();
            if ns_app != nil {
                let dock_tile: id = msg_send![ns_app, dockTile];
                if dock_tile != nil {
                    let label = if enabled && approval_count > 0 {
                        approval_count.to_string()
                    } else {
                        String::new()
                    };
                    let ns_label = NSString::alloc(nil).init_str(&label).autorelease();
                    let _: () = msg_send![dock_tile, setBadgeLabel: ns_label];
                }
            }
            pool.drain();
        }
    }
}

pub fn init_notifications(click_tx: UnboundedSender<String>) {
    if let Ok(mut guard) = click_slot().lock() {
        *guard = Some(click_tx);
    }
    #[cfg(target_os = "macos")]
    macos_impl::init();
}

pub fn deliver_approval_notification(target: &ApprovalNotificationTarget) {
    #[cfg(target_os = "macos")]
    macos_impl::deliver(target);
    #[cfg(not(target_os = "macos"))]
    let _ = target;
}

pub fn sync_dock_badge(approval_count: usize, enabled: bool) {
    #[cfg(target_os = "macos")]
    macos_impl::sync_badge(approval_count, enabled);
    #[cfg(not(target_os = "macos"))]
    let _ = (approval_count, enabled);
}

#[cfg(test)]
mod tests {
    use afterlife_core::{JobId, StageId};

    use super::*;

    #[test]
    fn non_bundle_test_process_safely_noops_all_notification_and_badge_calls() {
        assert!(!has_macos_app_bundle());
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        init_notifications(tx);
        deliver_approval_notification(&ApprovalNotificationTarget {
            job_id: JobId::from("JOB-999"),
            job_title: "Guard test".into(),
            district_name: "DIST-001".into(),
            stage_id: StageId::from("approve"),
            entry_key: "DIST-001-approve-1".into(),
        });
        sync_dock_badge(3, true);
        sync_dock_badge(0, false);
    }
}
