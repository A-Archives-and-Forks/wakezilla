use leptos::prelude::*;

pub mod api;
pub mod components;
pub mod models;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use components::{HomePage, MachineDetailPage};

#[component]
fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Html attr:lang="en" />
        <Stylesheet id="leptos" href="/style/main.css" />
        <Title text="Wakezilla" />
        <Router>
            <main class="container">
                <Routes fallback=|| "Page not found">
                    <Route path=path!("/") view=HomePage />
                    <Route path=path!("/machines/:mac") view=MachineDetailPage />
                </Routes>
            </main>
        </Router>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App)
}
