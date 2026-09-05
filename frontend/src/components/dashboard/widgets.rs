use super::state::{DashboardState, MachineStatus};
use crate::models::MachineType;
use leptos::prelude::*;

pub const MACHINE_TYPES: [(MachineType, &str, &str); 6] = [
    (MachineType::Server, "server", "Server"),
    (MachineType::Nas, "nas", "NAS"),
    (MachineType::Computer, "computer", "Computer"),
    (MachineType::MiniPc, "mini_pc", "Mini PC"),
    (MachineType::RaspberryPi, "raspberry_pi", "Raspberry Pi"),
    (MachineType::Notebook, "notebook", "Notebook"),
];
pub fn type_value(kind: MachineType) -> &'static str {
    MACHINE_TYPES
        .iter()
        .find(|(value, _, _)| *value == kind)
        .map(|(_, value, _)| *value)
        .unwrap_or("server")
}
pub fn parse_type(value: &str) -> MachineType {
    MACHINE_TYPES
        .iter()
        .find(|(_, key, _)| *key == value)
        .map(|(value, _, _)| *value)
        .unwrap_or_default()
}

#[component]
pub fn Icon(name: &'static str) -> impl IntoView {
    view! { <svg class="icon" aria-hidden="true"><use href=format!("#i-{name}")/></svg> }
}
#[component]
pub fn Hardware(kind: MachineType) -> impl IntoView {
    let asset = match kind {
        MachineType::Server => "server",
        MachineType::Nas => "nas",
        MachineType::Computer => "gaming",
        MachineType::MiniPc => "studio",
        MachineType::RaspberryPi => "pi",
        MachineType::Notebook => "laptop",
    };
    view! { <img class="hardware-art" src=format!("/images/hardware/{asset}.svg") alt="" width="80" height="80"/> }
}
#[component]
pub fn Status(state: DashboardState, mac: String) -> impl IntoView {
    let mac = StoredValue::new(mac);
    let pending = move || {
        state
            .pending
            .with(|pending| mac.with_value(|mac| pending.get(mac).copied()))
    };
    let current = move || mac.with_value(|mac| state.status(mac));
    view! { <span class=move || if pending().is_some() { "machine-status waking" } else if current() == MachineStatus::Online { "machine-status" } else { "machine-status offline" }>
        <span class=move || if current() == MachineStatus::Online { "status-dot green" } else { "status-dot muted" }></span>
        {move || pending().map(|wake| if wake { "Waking..." } else { "Shutting down..." }).unwrap_or_else(|| current().label())}
    </span> }
}
#[component]
pub fn ModalHeader(
    title: Signal<String>,
    subtitle: &'static str,
    eyebrow: &'static str,
    state: DashboardState,
) -> impl IntoView {
    view! { <div class="modal-header"><div><p class="eyebrow">{eyebrow}</p><h2 id="modal-title" tabindex="-1">{title}</h2><p>{subtitle}</p></div>
        <button type="button" class="icon-button" aria-label="Close window" on:click=move |_| state.dialog.set(None)><Icon name="close"/></button>
    </div> }
}
#[component]
pub fn ErrorMessage(message: Signal<Option<String>>) -> impl IntoView {
    view! { <Show when=move || message.get().is_some()><p class="form-error" role="alert">{move || message.get()}</p></Show> }
}
