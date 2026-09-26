// A hook: whether the system is in dark mode, rendering again when it changes.
// In JS it's `useDarkMode`, the name React finds a hook by (ADR 0046).

use react::{Notify, use_sync_external_store};
use web::{MediaQueryList, abort_controller, media_query_list, window};

use crate::listen::listen;

thread_local! {
    static DARK: &'static MediaQueryList = window::match_media(window, "(prefers-color-scheme: dark)");
}

pub fn use_dark_mode() -> bool {
    *use_sync_external_store(
        |notify: Notify| {
            let controller = abort_controller::new();
            listen(DARK.with(|dark| *dark), "change", Box::new(move |_| notify.call()), controller);
            move || abort_controller::abort(controller)
        },
        || media_query_list::matches(DARK.with(|dark| *dark)),
    )
}
