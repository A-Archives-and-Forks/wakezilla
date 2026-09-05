mod browser;
mod history;
mod machine;
mod scan;
mod setup;
mod state;
mod widgets;

use crate::{api, models::Machine};
use leptos::prelude::*;
use state::{DashboardState, DialogView, MachineStatus};
use wasm_bindgen::JsCast;
use widgets::{Hardware, Icon, Status};

pub fn new_machine() -> Machine {
    Machine {
        can_be_turned_off: true,
        turn_off_port: Some(3001),
        ..Default::default()
    }
}

#[component]
pub fn HomePage() -> impl IntoView {
    let state = DashboardState::new();
    let search = RwSignal::new(String::new());
    let filter = RwSignal::new("all");
    let list_view = RwSignal::new(false);
    let activity = RwSignal::new(false);
    let light = RwSignal::new(browser::initial_theme());
    let now = RwSignal::new(chrono::Local::now());
    let opened_route = RwSignal::new(false);
    let params = leptos_router::hooks::use_params_map();
    let dialog = NodeRef::<leptos::html::Dialog>::new();
    let search_input = NodeRef::<leptos::html::Input>::new();

    Effect::new(move |_| {
        state.refresh.get();
        let route_mac = params.with(|params| params.get("mac"));
        state.loading.set(true);
        state.error.set(None);
        state.spawn(async move {
            match api::fetch_machines().await {
                Ok(machines) => {
                    state.machines.set(machines);
                    if !opened_route.get_untracked()
                        && let Some(mac) = route_mac
                    {
                        opened_route.set(true);
                        match api::get_details_machine(&mac).await {
                            Ok(machine) => state.open(machine, false),
                            Err(error) => state.notify(error, true),
                        }
                    }
                }
                Err(error) => state.error.set(Some(error)),
            }
            state.loading.set(false);
        });
    });
    state.spawn(async move {
        loop {
            let epoch = state.status_epoch.get_untracked();
            let machines = state.machines.get_untracked();
            let statuses =
                futures::future::join_all(machines.into_iter().map(|machine| async move {
                    let status = match api::get_machine_status(&machine.mac).await {
                        Ok(true) => MachineStatus::Online,
                        Ok(false) => MachineStatus::Unreachable,
                        Err(_) => MachineStatus::Error,
                    };
                    (machine.mac, status)
                }))
                .await;
            if epoch == state.status_epoch.get_untracked() {
                state.statuses.set(statuses.into_iter().collect());
            }
            gloo_timers::future::TimeoutFuture::new(5000).await;
        }
    });
    state.spawn(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(30000).await;
            now.set(chrono::Local::now());
        }
    });
    Effect::new(move |_| {
        let open = state.dialog.get().is_some();
        if let Some(dialog) = dialog.get() {
            if open && !dialog.open() {
                let _ = dialog.show_modal();
                browser::focus_dialog();
            } else if !open && dialog.open() {
                dialog.close();
            }
        }
    });
    let online = move || {
        state.machines.with(|machines| {
            machines
                .iter()
                .filter(|machine| state.status(&machine.mac) == MachineStatus::Online)
                .count()
        })
    };
    let offline = move || {
        state.machines.with(|machines| {
            machines
                .iter()
                .filter(|machine| state.status(&machine.mac) == MachineStatus::Unreachable)
                .count()
        })
    };
    let visible = Memo::new(move |_| {
        let query = search.get().trim().to_lowercase();
        state
            .machines
            .get()
            .into_iter()
            .filter(|machine| {
                let status = state.status(&machine.mac);
                let matches_status = match filter.get() {
                    "online" => status == MachineStatus::Online,
                    "offline" => status == MachineStatus::Unreachable,
                    _ => true,
                };
                matches_status
                    && (query.is_empty()
                        || format!(
                            "{} {} {}",
                            machine.name,
                            machine.ip,
                            machine
                                .port_forwards
                                .iter()
                                .filter_map(|service| service.name.as_deref())
                                .collect::<Vec<_>>()
                                .join(" ")
                        )
                        .to_lowercase()
                        .contains(&query))
            })
            .collect::<Vec<_>>()
    });
    view! {
        <div class="landscape" aria-hidden="true"></div>
        <div class="app-shell" on:keydown=move |event: web_sys::KeyboardEvent| {
            let editing = event.target().and_then(|target| target.dyn_into::<web_sys::Element>().ok()).is_some_and(|element| matches!(element.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT") || element.has_attribute("contenteditable"));
            if event.key() == "/" && !editing && state.dialog.get_untracked().is_none() { event.prevent_default(); activity.set(false); if let Some(input) = search_input.get() { let _ = input.focus(); } }
        }>
            <header class="topbar">
                <a class="brand" href="https://wakezilla.dev" aria-label="Wakezilla website"><img src="/images/wakezilla.png" alt="" width="47" height="40"/><span>"wakezilla"<span class="brand-period">"."</span></span></a>
                <div class="topbar-actions"><button class="icon-button" id="theme-toggle" aria-label="Light theme" aria-pressed=move || light.get().to_string() title=move || if light.get() { "Use dark theme" } else { "Use light theme" } on:click=move |_| { light.update(|value| *value = !*value); browser::apply_theme(light.get_untracked()); }>{move || view! { <Icon name=if light.get() { "moon" } else { "sun" }/> }}</button></div>
            </header>
            <main class="workspace">
                <aside class="sidebar" aria-label="Overview">
                    <div class="clock-block"><time>{move || now.get().format("%H:%M").to_string()}</time><p>{move || now.get().format("%d/%m/%Y").to_string()}</p></div>
                    <section class="glass summary-panel">
                        <div class="panel-title"><Icon name="network"/><h2>"Overview"</h2></div>
                        <div class="overview-ring"><svg viewBox="0 0 120 120" aria-hidden="true"><circle class="ring-track" cx="60" cy="60" r="49"/><circle class="ring-progress" cx="60" cy="60" r="49" style:stroke-dashoffset=move || (307.9 * (1.0 - online() as f64 / state.machines.get().len().max(1) as f64)).to_string()/></svg><div><strong>{online}<span>{move || format!("/{}", state.machines.get().len())}</span></strong><span>"machines online"</span></div></div>
                        <div class="summary-stats"><div><span class="status-dot green"></span>"Online"<strong>{online}</strong></div><div><span class="status-dot muted"></span>"Unreachable"<strong>{offline}</strong></div><div><Icon name="zap"/>"Configured services"<strong>{move || state.machines.with(|machines| machines.iter().map(|machine| machine.port_forwards.len()).sum::<usize>())}</strong></div></div>
                        <Show when={move || state.machines.get().len() > online() + offline()}><p class="summary-note">{move || format!("{} without a confirmed status", state.machines.get().len() - online() - offline())}</p></Show>
                    </section>
                </aside>
                <div class="main-content">
                    <div class="content-navigation"><div class="page-tabs" role="tablist" aria-label="Content" on:keydown=move |event: web_sys::KeyboardEvent| {
                        if matches!(event.key().as_str(), "ArrowLeft" | "ArrowRight" | "Home" | "End") {
                            event.prevent_default(); activity.set(match event.key().as_str() { "Home"=>false,"End"=>true,_=>!activity.get_untracked() });
                            if let Some(element) = web_sys::window().and_then(|window| window.document()).and_then(|document| document.get_element_by_id(if activity.get_untracked() { "activity-tab" } else { "machines-tab" })).and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok()) { let _ = element.focus(); }
                        }
                    }>
                        <button class=move || if activity.get() { "page-tab" } else { "page-tab active" } id="machines-tab" role="tab" aria-controls="machines-panel" aria-selected=move || (!activity.get()).to_string() tabindex=move || if activity.get() { -1 } else { 0 } on:click=move |_| activity.set(false)><Icon name="grid"/>"Machines"<span>{move || state.machines.get().len()}</span></button>
                        <button class=move || if activity.get() { "page-tab active" } else { "page-tab" } id="activity-tab" role="tab" aria-controls="activity-panel" aria-selected=move || activity.get().to_string() tabindex=move || if activity.get() { 0 } else { -1 } on:click=move |_| activity.set(true)><Icon name="activity"/>"Activity"</button>
                    </div><button class="button button-primary" id="add-button" on:click=move |_| state.open(new_machine(), true)><Icon name="plus"/>"Add machine"</button></div>
                    <section id="machines-panel" role="tabpanel" aria-labelledby="machines-tab" hidden=move || activity.get()>
                        <div class="toolbar"><label class="search-field"><Icon name="search"/><input node_ref=search_input type="search" placeholder="Search machines..." aria-label="Search by machine name, IP address, or service" prop:value=move || search.get() on:input=move |event| search.set(event_target_value(&event))/><kbd>"/"</kbd></label><button class="button scan-button" on:click=move |_| state.dialog.set(Some(DialogView::Scan))><Icon name="scan"/><span>"Find on network"</span></button></div>
                        <div class="filter-row"><div class="filters" role="group" aria-label="Filter machines">{[("all","All"),("online","Online"),("offline","Unreachable")].into_iter().map(|(value,label)| view! { <button class=move || if filter.get() == value { "filter active" } else { "filter" } aria-pressed=move || (filter.get() == value).to_string() on:click=move |_| filter.set(value)>{label}</button> }).collect_view()}</div><div class="view-switch" role="group" aria-label="View"><button class=move || if list_view.get() { "icon-button" } else { "icon-button selected" } aria-label="Grid view" aria-pressed=move || (!list_view.get()).to_string() on:click=move |_| list_view.set(false)><Icon name="grid"/></button><button class=move || if list_view.get() { "icon-button selected" } else { "icon-button" } aria-label="List view" aria-pressed=move || list_view.get().to_string() on:click=move |_| list_view.set(true)><Icon name="list"/></button></div></div>
                        <Show when=move || state.error.get().is_some()><div class="glass empty-state" role="alert"><h2>"Could not load machines"</h2><p>{move || state.error.get()}</p><button class="button" on:click=move |_| state.refresh.update(|value| *value += 1)>"Try again"</button></div></Show>
                        <Show when=move || state.loading.get()><p class="loading-state" role="status">"Loading machines..."</p></Show>
                        <div class=move || if list_view.get() { "machine-grid list-view" } else { "machine-grid" }><For each=move || visible.get() key=|machine| machine.mac.clone() children=move |machine| view! { <MachineCard state initial=machine/> }/></div>
                        <Show when=move || !state.loading.get() && state.error.get().is_none() && visible.get().is_empty()><div class="glass empty-state"><Icon name="server"/><h2>{move || if state.machines.get().is_empty() { "Add your first machine" } else { "No machines found" }}</h2><p>{move || if state.machines.get().is_empty() { "Add a machine or find devices on the network." } else { "Try another name, IP address, or filter." }}</p><button class="button" on:click=move |_| { if state.machines.get_untracked().is_empty() { state.open(new_machine(), true); } else { search.set(String::new()); filter.set("all"); } }>{move || if state.machines.get().is_empty() { "Add machine" } else { "Clear search and filters" }}</button></div></Show>
                        <div class="grid-caption"><span><Icon name="info"/>"Select a machine to view its services and controls."</span><span>{move || format!("{} machines", visible.get().len())}</span></div>
                    </section>
                    <section class="glass activity-panel" id="activity-panel" role="tabpanel" aria-labelledby="activity-tab" hidden=move || !activity.get()><div class="activity-heading"><h2>"Recent events"</h2><span class="small-label">"THIS SESSION"</span></div><Show when=move || state.events.get().is_empty()><p class="services-empty">"Actions from this dashboard appear here. Access history is available in each machine's details."</p></Show>{move || state.events.get().into_iter().map(|event| view! { <div class="activity-item"><span class="event-symbol"><Icon name="activity"/></span><div><p><strong>{event.machine}</strong>" "{event.message}</p></div><time>{event.time}</time></div> }).collect_view()}</section>
                    {move || state.events.get().first().cloned().map(|event| view! { <section class="glass recent-event" aria-label="Latest activity"><span class="event-symbol"><Icon name="activity"/></span><div><span class="small-label">"LATEST ACTIVITY THIS SESSION"</span><p><strong>{event.machine}</strong>" "{event.message}</p></div><button aria-label="View all activity" on:click=move |_| activity.set(true)><span>"View activity"</span><Icon name="arrow"/></button></section> })}
                </div>
            </main>
            <footer class="footer"><span>"Wakezilla · Wake-on-LAN"</span></footer>
            <dialog node_ref=dialog aria-labelledby="modal-title" data-view=move || if matches!(state.dialog.get(), Some(DialogView::Scan)) { "scan" } else { "detail" } on:cancel=move |event: web_sys::Event| { event.prevent_default(); state.dialog.set(None); } on:close=move |_| state.dialog.set(None) on:click=move |event| { if event.target().is_some_and(|target| target.dyn_ref::<web_sys::HtmlDialogElement>().is_some()) { state.dialog.set(None); } }>
                <div id="modal-content">{move || state.dialog.get().map(|view| match view {
                    DialogView::Machine {machine,creating} => view! { <machine::MachineDialog state initial=*machine creating/> }.into_any(),
                    DialogView::Scan => view! { <scan::ScanDialog state/> }.into_any(),
                })}</div>
            </dialog>
            <div class=move || if state.toast.get().is_some() { "toast visible" } else { "toast" } class:error-toast=move || state.toast.get().is_some_and(|(_,error)| error) role="status" aria-live="polite" aria-atomic="true">{move || state.toast.get().map(|(message,_)| message)}</div>
        </div>
    }
}

#[component]
fn MachineCard(state: DashboardState, initial: Machine) -> impl IntoView {
    let mac = StoredValue::new(initial.mac.clone());
    let current = Memo::new(move |_| {
        state
            .machines
            .with(|machines| {
                machines
                    .iter()
                    .find(|machine| mac.with_value(|mac| *mac == machine.mac))
                    .cloned()
            })
            .unwrap_or_else(|| initial.clone())
    });
    let pending = move || {
        state
            .pending
            .with(|pending| mac.with_value(|mac| pending.contains_key(mac)))
    };
    let online = move || mac.with_value(|mac| state.status(mac) == MachineStatus::Online);
    view! { <article class=move || if online() { "glass machine-card online" } else { "glass machine-card offline" } aria-label=move || current.get().name>
        <div class="card-top"><Status state mac=mac.get_value()/><button class="icon-button" aria-label=move || format!("Details for {}", current.get().name) on:click=move |_| state.open(current.get_untracked(), false)><Icon name="more"/></button></div>
        <button class="machine-identity" aria-label=move || format!("Open {}", current.get().name) on:click=move |_| state.open(current.get_untracked(), false)>{move || view! { <Hardware kind=current.get().machine_type/> }}<span class="machine-name">{move || current.get().name}</span><span class="machine-ip">{move || current.get().ip}</span></button>
        <div class="service-list">{move || current.get().port_forwards.into_iter().map(|service| view! { <span class="service-chip"><Icon name="server"/>{service.name.filter(|name| !name.is_empty()).unwrap_or_else(|| format!("Port {}", service.local_port))}</span> }).collect_view()}<Show when=move || current.get().port_forwards.is_empty()><span class="small-label">"NO SERVICES"</span></Show></div>
        <div class="card-footer"><span>{move || if current.get().inactivity_period == 0 { "Automatic shutdown disabled".to_owned() } else { format!("Inactivity: {} min", current.get().inactivity_period) }}</span>
            <Show when=online fallback=move || view! { <button class="card-action wake" disabled=pending on:click=move |_| state.power(current.get_untracked(), true)><Icon name="power"/>{move || if pending() { "Please wait..." } else { "Wake machine" }}</button> }><button class="card-action" on:click=move |_| state.open(current.get_untracked(), false)>"View details"<Icon name="arrow"/></button></Show>
        </div>
    </article> }
}

#[cfg(test)]
mod tests {
    #[test]
    fn new_machines_default_to_sixty_minutes_of_inactivity() {
        assert_eq!(super::new_machine().inactivity_period, 60);
    }
}
