//! The pilot's client (ROADMAP M3.3): a contacts app. Its models and their
//! rules are the server's (`models`), and it's compiled by rust-js, as Cargo
//! checks the workspace (ADR 0101).

#![allow(non_snake_case)]

mod api;
mod detail;
mod form;
mod list;
mod route;
mod sonner;

use detail::ContactPage;
use form::NewContactForm;
use list::ContactList;
use react::Element;
use route::{Route, use_route};
use sonner::Toaster;

pub fn App() -> Element {
    let page = match use_route() {
        Route::List => jsx! { <ContactList /> },
        Route::Contact(id) => jsx! { <ContactPage key={id} id={id} /> },
        Route::New => jsx! { <NewContactForm /> },
        Route::NotFound => jsx! { <p className="error">{"There's no such page."}</p> },
    };
    jsx! {
        <div className="app">
            <nav>
                <a href="#/">{"Contacts"}</a>
                <a href="#/new">{"New contact"}</a>
            </nav>
            <main>{page}</main>
            <Toaster position="bottom-right" />
        </div>
    }
}
