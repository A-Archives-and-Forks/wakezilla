use leptos::prelude::*;

pub mod api;
pub mod components;
pub mod models;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use components::HomePage;

#[component]
fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Html attr:lang="en" />
        <Title text="Wakezilla" />
        <Router>
                <Routes fallback=|| view! { <p>"Page not found. "<a href="/">"Return to dashboard"</a></p> }>
                    <Route path=path!("/") view=HomePage />
                    <Route path=path!("/machines/:mac") view=HomePage />
                </Routes>
        </Router>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App)
}
