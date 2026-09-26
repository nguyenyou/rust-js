//! [React's events](https://react.dev/reference/react-dom/components/common#react-event-object),
//! which wrap the DOM's. Each derefs to the one it extends, as React's do:
//! a [`Pointer`] is a [`Mouse`], which is a [`Ui`], which is an [`Event`].

use core::marker::PhantomData;
use core::ops::Deref;

use web::JsObject;

/// A [React event](https://react.dev/reference/react-dom/components/common#react-event-object):
/// what every handler gets.
pub struct Event(PhantomData<JsObject>);

impl Event {
    /// Stop the browser's default action, like submitting a form.
    #[rust_js::link_name = "preventDefault"]
    pub fn prevent_default(&self) {
        unreachable!()
    }

    /// Stop parents' handlers seeing it.
    #[rust_js::link_name = "stopPropagation"]
    pub fn stop_propagation(&self) {
        unreachable!()
    }

    #[rust_js::link_name = "isDefaultPrevented"]
    pub fn is_default_prevented(&self) -> bool {
        unreachable!()
    }

    #[rust_js::link_name = "isPropagationStopped"]
    pub fn is_propagation_stopped(&self) -> bool {
        unreachable!()
    }
}

/// Getters, one per React event field: `client_x` is `e.clientX`.
macro_rules! fields {
    ($type:ident { $($(#[doc = $doc:literal])* $method:ident: $ty:ty = $js:literal;)* }) => {
        impl $type {
            $(
                $(#[doc = $doc])*
                #[rust_js::link_name = concat!("get ", $js)]
                pub fn $method(&self) -> $ty {
                    unreachable!()
                }
            )*
        }
    };
}

fields!(Event {
    bubbles: bool = "bubbles";
    cancelable: bool = "cancelable";
    /// The element whose handler this is.
    current_target: &'static web::Element = "currentTarget";
    default_prevented: bool = "defaultPrevented";
    event_phase: u32 = "eventPhase";
    is_trusted: bool = "isTrusted";
    /// Where it happened.
    target: &'static web::Element = "target";
    time_stamp: f64 = "timeStamp";
    /// The DOM's event that this wraps.
    native_event: &'static web::Event = "nativeEvent";
    /// Its name, like `"click"`.
    type_: String = "type";
});

/// Declares an event type that extends another.
macro_rules! events {
    ($($(#[doc = $doc:literal])* $name:ident: $parent:ident { $($body:tt)* })*) => {
        $(
            $(#[doc = $doc])*
            pub struct $name(PhantomData<JsObject>);

            impl Deref for $name {
                type Target = $parent;

                fn deref(&self) -> &$parent {
                    // Never runs: rust-js compiles this `Deref` to the object itself.
                    unsafe { &*(self as *const Self as *const $parent) }
                }
            }

            fields!($name { $($body)* });
        )*
    };
}

events! {
    /// A [UI event](https://developer.mozilla.org/docs/Web/API/UIEvent), like a scroll.
    Ui: Event {
        detail: i32 = "detail";
        view: &'static web::Window = "view";
    }

    /// A click, or another [mouse event](https://developer.mozilla.org/docs/Web/API/MouseEvent).
    Mouse: Ui {
        alt_key: bool = "altKey";
        /// Which button: 0 is the main one.
        button: i32 = "button";
        buttons: i32 = "buttons";
        client_x: f64 = "clientX";
        client_y: f64 = "clientY";
        ctrl_key: bool = "ctrlKey";
        meta_key: bool = "metaKey";
        movement_x: f64 = "movementX";
        movement_y: f64 = "movementY";
        page_x: f64 = "pageX";
        page_y: f64 = "pageY";
        related_target: Option<&'static web::Element> = "relatedTarget";
        screen_x: f64 = "screenX";
        screen_y: f64 = "screenY";
        shift_key: bool = "shiftKey";
    }

    /// A [pointer event](https://developer.mozilla.org/docs/Web/API/PointerEvent):
    /// mouse, pen or touch.
    Pointer: Mouse {
        height: f64 = "height";
        is_primary: bool = "isPrimary";
        pointer_id: i32 = "pointerId";
        /// `"mouse"`, `"pen"` or `"touch"`.
        pointer_type: String = "pointerType";
        pressure: f64 = "pressure";
        tangential_pressure: f64 = "tangentialPressure";
        tilt_x: f64 = "tiltX";
        tilt_y: f64 = "tiltY";
        twist: f64 = "twist";
        width: f64 = "width";
    }

    /// A [drag event](https://developer.mozilla.org/docs/Web/API/DragEvent).
    /// Call `prevent_default` in `on_drag_over` to allow a drop.
    Drag: Mouse {
        data_transfer: &'static web::DataTransfer = "dataTransfer";
    }

    /// A [wheel event](https://developer.mozilla.org/docs/Web/API/WheelEvent).
    Wheel: Mouse {
        delta_mode: u32 = "deltaMode";
        delta_x: f64 = "deltaX";
        delta_y: f64 = "deltaY";
        delta_z: f64 = "deltaZ";
    }

    /// Focus coming or going: `on_focus` and `on_blur`, which bubble in React.
    Focus: Ui {
        /// Where focus went, or came from.
        related_target: Option<&'static web::Element> = "relatedTarget";
    }

    /// A key pressed or let go.
    Keyboard: Ui {
        alt_key: bool = "altKey";
        /// Which key it is on the keyboard, like `"KeyA"`.
        code: String = "code";
        ctrl_key: bool = "ctrlKey";
        /// What the key means, like `"Enter"` or `"a"`.
        key: String = "key";
        locale: String = "locale";
        location: u32 = "location";
        meta_key: bool = "metaKey";
        repeat: bool = "repeat";
        shift_key: bool = "shiftKey";
    }

    /// A [touch event](https://developer.mozilla.org/docs/Web/API/TouchEvent).
    Touch: Ui {
        alt_key: bool = "altKey";
        changed_touches: &'static web::TouchList = "changedTouches";
        ctrl_key: bool = "ctrlKey";
        meta_key: bool = "metaKey";
        shift_key: bool = "shiftKey";
        target_touches: &'static web::TouchList = "targetTouches";
        touches: &'static web::TouchList = "touches";
    }

    /// A CSS [animation event](https://developer.mozilla.org/docs/Web/API/AnimationEvent).
    Animation: Event {
        animation_name: String = "animationName";
        elapsed_time: f64 = "elapsedTime";
        pseudo_element: String = "pseudoElement";
    }

    /// A CSS [transition event](https://developer.mozilla.org/docs/Web/API/TransitionEvent).
    Transition: Event {
        elapsed_time: f64 = "elapsedTime";
        property_name: String = "propertyName";
        pseudo_element: String = "pseudoElement";
    }

    /// Copying, cutting or pasting.
    Clipboard: Event {
        clipboard_data: &'static web::DataTransfer = "clipboardData";
    }

    /// Text being composed with an input method.
    Composition: Event {
        data: String = "data";
    }

    /// `on_before_input`: text about to be typed.
    Input: Event {
        data: Option<String> = "data";
    }

    /// A popover or `<details>` opening or closing: `on_toggle`, `on_before_toggle`.
    Toggle: Event {
        /// `"open"` or `"closed"`.
        new_state: String = "newState";
        old_state: String = "oldState";
    }

    /// An `<input>`, `<select>` or `<textarea>` changing: `on_change`, `on_input`.
    Change: Event {
        /// What's in it now: `e.target.value`.
        value: String = "target.value";
        /// Whether a checkbox is checked now: `e.target.checked`.
        checked: bool = "target.checked";
    }
}

impl Mouse {
    /// Whether a modifier key, like `"Shift"` or `"CapsLock"`, is down.
    #[rust_js::link_name = "getModifierState"]
    pub fn get_modifier_state(&self, key: &str) -> bool {
        unreachable!()
    }
}

impl Keyboard {
    #[rust_js::link_name = "getModifierState"]
    pub fn get_modifier_state(&self, key: &str) -> bool {
        unreachable!()
    }
}
