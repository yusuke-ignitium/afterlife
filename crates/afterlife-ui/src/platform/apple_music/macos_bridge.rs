use std::{ffi::CStr, os::raw::c_char, sync::Once};

use cocoa::{
    base::{BOOL, NO, id, nil},
    foundation::{NSAutoreleasePool, NSDictionary, NSString},
};
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};

use super::{AppleMusicWatcher, MusicTrackEvent, handle_notification_event};

const OBSERVER_CLASS_NAME: &str = "AfterlifeMusicNotificationObserver";
const MUSIC_NOTIFICATION_NAME: &str = "com.apple.Music.playerInfo";
const MUSIC_BUNDLE_ID: &str = "com.apple.Music";

static REGISTER_CLASS: Once = Once::new();

fn observer_class() -> &'static Class {
    REGISTER_CLASS.call_once(|| {
        let superclass = class!(NSObject);
        if let Some(mut decl) = ClassDecl::new(OBSERVER_CLASS_NAME, superclass) {
            unsafe {
                decl.add_method(
                    sel!(onMusicPlayerInfo:),
                    on_music_player_info as extern "C" fn(&Object, Sel, id),
                );
            }
            decl.register();
        }
    });
    Class::get(OBSERVER_CLASS_NAME).expect("registered observer class")
}

unsafe fn ns_dict_string(dict: id, key: &str) -> Option<String> {
    if dict == nil {
        return None;
    }
    let ns_key: id = unsafe { NSString::alloc(nil).init_str(key).autorelease() };
    let val: id = unsafe { dict.objectForKey_(ns_key) };
    if val == nil {
        return None;
    }
    let is_str: BOOL = unsafe { msg_send![val, isKindOfClass: class!(NSString)] };
    let str_obj: id = if is_str != NO {
        val
    } else {
        let responds: BOOL = unsafe { msg_send![val, respondsToSelector: sel!(stringValue)] };
        if responds != NO {
            unsafe { msg_send![val, stringValue] }
        } else {
            nil
        }
    };
    if str_obj == nil {
        return None;
    }
    let ptr: *const c_char = unsafe { msg_send![str_obj, UTF8String] };
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .ok()
        .map(|s| s.to_string())
}

extern "C" fn on_music_player_info(_this: &Object, _cmd: Sel, notification: id) {
    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        if notification != nil {
            let user_info: id = msg_send![notification, userInfo];
            if user_info != nil {
                let player_state =
                    ns_dict_string(user_info, "Player State").unwrap_or_default();
                let is_playing = player_state.eq_ignore_ascii_case("Playing");
                let event = MusicTrackEvent {
                    is_playing,
                    name: ns_dict_string(user_info, "Name").unwrap_or_default(),
                    artist: ns_dict_string(user_info, "Artist").unwrap_or_default(),
                    album: ns_dict_string(user_info, "Album").unwrap_or_default(),
                    store_url: ns_dict_string(user_info, "Store URL")
                        .filter(|s| !s.trim().is_empty()),
                    persistent_id: ns_dict_string(user_info, "PersistentID")
                        .filter(|s| !s.trim().is_empty()),
                };
                handle_notification_event(event);
            }
        }
        pool.drain();
    }
}

pub fn is_music_app_running() -> bool {
    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        let bundle_id: id = NSString::alloc(nil).init_str(MUSIC_BUNDLE_ID).autorelease();
        let apps: id = msg_send![
            class!(NSRunningApplication),
            runningApplicationsWithBundleIdentifier: bundle_id
        ];
        let count: usize = if apps != nil {
            msg_send![apps, count]
        } else {
            0
        };
        pool.drain();
        count > 0
    }
}

pub fn register_observer(w: &AppleMusicWatcher) {
    let mut guard = w.observer_ptr.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_some() {
        return;
    }
    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        let cls = observer_class();
        let observer: id = msg_send![cls, new];
        if observer != nil {
            let center: id = msg_send![class!(NSDistributedNotificationCenter), defaultCenter];
            let name: id = NSString::alloc(nil)
                .init_str(MUSIC_NOTIFICATION_NAME)
                .autorelease();
            let _: () = msg_send![
                center,
                addObserver: observer
                selector: sel!(onMusicPlayerInfo:)
                name: name
                object: nil
            ];
            *guard = Some(observer as usize);
        }
        pool.drain();
    }
}

pub fn unregister_observer(w: &AppleMusicWatcher) {
    let mut guard = w.observer_ptr.lock().unwrap_or_else(|e| e.into_inner());
    let Some(ptr) = guard.take() else {
        return;
    };
    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        let observer = ptr as id;
        let center: id = msg_send![class!(NSDistributedNotificationCenter), defaultCenter];
        let _: () = msg_send![center, removeObserver: observer];
        let _: () = msg_send![observer, release];
        pool.drain();
    }
}
