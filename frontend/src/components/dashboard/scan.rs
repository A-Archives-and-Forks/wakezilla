use super::{
    new_machine,
    state::DashboardState,
    widgets::{ErrorMessage, Icon, ModalHeader},
};
use crate::{
    api,
    models::{DiscoveredDevice, NetworkInterface},
};
use leptos::prelude::*;

#[component]
pub fn ScanDialog(state: DashboardState) -> impl IntoView {
    let interfaces = RwSignal::<Vec<NetworkInterface>>::new(vec![]);
    let selected = RwSignal::new(String::new());
    let devices = RwSignal::<Vec<DiscoveredDevice>>::new(vec![]);
    let loading = RwSignal::new(true);
    let scanning = RwSignal::new(false);
    let searched = RwSignal::new(false);
    let error = RwSignal::new(None);
    leptos::task::spawn_local_scoped_with_cancellation(async move {
        match api::fetch_interfaces().await {
            Ok(values) => {
                if let Some(interface) = values
                    .iter()
                    .find(|interface| interface.is_up && !interface.ip.starts_with("127."))
                {
                    selected.set(interface.name.clone());
                }
                interfaces.set(values);
            }
            Err(message) => error.set(Some(message)),
        }
        loading.set(false);
    });
    view! {
        <ModalHeader state title=Signal::derive(|| "Find your machines".to_owned()) subtitle="Select the server network interface used to find devices." eyebrow="NETWORK DISCOVERY"/>
        <ErrorMessage message=error.into()/>
        <div class="field"><label for="scan-interface">"Network interface"</label><select id="scan-interface" disabled=move || loading.get() || scanning.get() prop:value=move || selected.get() on:change=move |event| selected.set(event_target_value(&event))><option value="">"Automatic selection"</option>{move || interfaces.get().into_iter().map(|interface| view! { <option value=interface.name.clone()>{format!("{} · {}", interface.name, interface.ip)}</option> }).collect_view()}</select></div>
        <div class="modal-actions"><button class="button button-primary" disabled=move || loading.get() || scanning.get() on:click=move |_| {
            scanning.set(true); error.set(None); devices.set(vec![]);
            leptos::task::spawn_local_scoped_with_cancellation(async move { match api::fetch_scan_network(selected.get_untracked()).await { Ok(values) => devices.set(values), Err(message) => error.set(Some(message)) }; scanning.set(false); searched.set(true); });
        }><Icon name="scan"/>{move || if scanning.get() { "Searching..." } else { "Find devices" }}</button></div>
        <Show when=move || scanning.get()><div class="discovery-state" role="status"><Icon name="scan"/><h3>"Searching the network..."</h3><p>"The search can take a few seconds."</p></div></Show>
        <Show when=move || searched.get() && !scanning.get() && devices.get().is_empty() && error.get().is_none()><p class="services-empty">"No devices found on this interface."</p></Show>
        {move || devices.get().into_iter().map(|device| {
            let registered = state.machines.with(|machines| machines.iter().any(|machine| machine.mac.eq_ignore_ascii_case(&device.mac)));
            let label = device.hostname.clone().filter(|name| !name.is_empty()).unwrap_or_else(|| "Unnamed device".into());
            view! { <div class="discover-card"><Icon name="server"/><div><strong>{label}</strong><small>{device.ip.clone()}</small><small>{device.mac.clone()}</small></div><button class="button" disabled=registered on:click=move |_| { let mut machine = new_machine(); machine.name = device.hostname.clone().unwrap_or_default(); machine.ip = device.ip.clone(); machine.mac = device.mac.clone(); state.open(machine, true); }>{if registered { "Already added" } else { "Add" }}</button></div> }
        }).collect_view()}
    }
}
