use leptos::prelude::*;
use std::collections::HashMap;

use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

use leptos::leptos_dom::logging::console_log;

use web_sys::{SubmitEvent, console};

use crate::api::create_machine;
use crate::components::ErrorDisplay;
use crate::models::{Machine, PortForward, validate_machine_form};

#[component]
pub fn AddMachine(
    machine: ReadSignal<Machine>,
    registred_machines: ReadSignal<Vec<Machine>>,
    set_registred_machines: WriteSignal<Vec<Machine>>,
) -> impl IntoView {
    let navigate = leptos_router::hooks::use_navigate();
    let (machine_form_data, set_machine_form_data) = signal::<Machine>(machine.get());
    let (show_turn_off_port, set_show_turn_off_port) = signal(false);
    let (port_forwards, set_port_forwards) = signal::<Vec<PortForward>>(vec![]);
    let (erros, set_errors) = signal::<HashMap<String, Vec<String>>>(HashMap::new());
    let (loading, set_loading) = signal(false);
    Effect::new(move |_| {
        let current = machine.get();
        set_show_turn_off_port.set(current.can_be_turned_off);
        console_log(&format!(
            "Pre-filling form with discovered machine: {:?} ({})",
            current.port_forwards.clone(),
            current.ip
        ));
        set_port_forwards.set(current.port_forwards.clone());
        set_machine_form_data.set(current);
    });

    Effect::new(move |_| {
        let forwards = port_forwards.get();
        set_machine_form_data.update(|machine| {
            machine.port_forwards = forwards.clone();
        });
    });

    fn set_input_value(
        key: &str,
        value: String,
        set_machine_form_data: WriteSignal<Machine>,
        machine_form_data: ReadSignal<Machine>,
        set_show_turn_off_port: WriteSignal<bool>,
    ) {
        let mut current = machine_form_data.get();
        match key {
            "name" => current.name = value,
            "mac" => current.mac = value,
            "ip" => current.ip = value,
            "description" => current.description = Some(value),
            "turn_off_port" => {
                let trimmed = value.trim();
                current.turn_off_port = if trimmed.is_empty() {
                    None
                } else {
                    trimmed.parse().ok()
                };
            }
            "can_be_turned_off" => {
                let enabled = value == "on";
                current.can_be_turned_off = enabled;
                if !enabled {
                    current.turn_off_port = None;
                }
                set_show_turn_off_port.set(enabled);
            }
            _ => {}
        };
        set_machine_form_data.set(current);
    }

    let on_submit = move |ev: SubmitEvent| {
        // stop the page from reloading!
        ev.prevent_default();

        set_loading.set(true);
        let navigate = navigate.clone();
        let validation_errors = validate_machine_form(&machine_form_data.get());
        if validation_errors.is_empty() {
            console::log_1(&"Form is valid".into());
            let current_machines = registred_machines.get();
            let mut new_machines = current_machines.clone();

            leptos::task::spawn_local(async move {
                if (create_machine(machine_form_data.get()).await).is_ok() {
                    //console_log(&format!("Loaded {:?} machines", machines));
                    let new_machine = machine_form_data.get();
                    new_machines.insert(0, new_machine.clone());

                    set_registred_machines.set(new_machines);
                    // Clear the form
                    set_machine_form_data.set(Machine {
                        name: "".to_string(),
                        mac: "".to_string(),
                        ip: "".to_string(),
                        description: None,
                        turn_off_port: None,
                        can_be_turned_off: false,
                        inactivity_period: 60,
                        port_forwards: vec![],
                    });
                    set_port_forwards.set(vec![]);
                    set_show_turn_off_port.set(false);
                    set_errors.set(HashMap::new());
                    let url = format!("/machines/{}", new_machine.mac);
                    navigate(&url, Default::default());
                } else {
                    console_log("Error creating machine");
                }
            });
            set_loading.set(false);
        } else {
            set_errors.set(validation_errors);
            set_loading.set(false);
        }
    };

    view! {
        <section class="card">
            <header class="card-header">
                <h3 class="card-title">"Add new machine"</h3>
                <p class="card-subtitle">
                    {move || {
                        let ip_hint = machine_form_data.get().ip;
                        if ip_hint.is_empty() {
                            "Register a device to make it available for wake and forwarding."
                                .to_string()
                        } else {
                            format!("Pre-filled from discovery: {}", ip_hint)
                        }
                    }}
                </p>
            </header>
            <form on:submit=on_submit class="form-grid">
                <div class="form-grid two-column">
                    <div class="field">
                        <label for="name">"Name"</label>
                        <input
                            type="text"
                            id="name"
                            name="name"
                            class="input"
                            required
                            on:input:target=move |ev| {
                                let input_value = ev.target().value();
                                set_input_value(
                                    "name",
                                    input_value,
                                    set_machine_form_data,
                                    machine_form_data,
                                    set_show_turn_off_port,
                                );
                            }
                            prop:value=move || machine_form_data.get().name
                        />
                        <ErrorDisplay erros=erros key="name" />
                    </div>
                    <div class="field">
                        <label for="mac">"MAC address"</label>
                        <input
                            type="text"
                            id="mac"
                            name="mac"
                            class="input"
                            required
                            on:input:target=move |ev| {
                                let input_value = ev.target().value();
                                set_input_value(
                                    "mac",
                                    input_value,
                                    set_machine_form_data,
                                    machine_form_data,
                                    set_show_turn_off_port,
                                );
                            }
                            prop:value=move || machine_form_data.get().mac
                        />
                        <ErrorDisplay erros=erros key="mac" />
                    </div>
                </div>

                <div class="form-grid two-column">
                    <div class="field">
                        <label for="ip">"IP address"</label>
                        <input
                            type="text"
                            id="ip"
                            name="ip"
                            class="input"
                            required
                            on:input:target=move |ev| {
                                let input_value = ev.target().value();
                                set_input_value(
                                    "ip",
                                    input_value,
                                    set_machine_form_data,
                                    machine_form_data,
                                    set_show_turn_off_port,
                                );
                            }
                            prop:value=move || machine_form_data.get().ip
                        />
                        <ErrorDisplay erros=erros key="ip" />
                    </div>
                </div>

                <div class="field">
                    <label for="description">"Description (optional)"</label>
                    <input
                        type="text"
                        id="description"
                        name="description"
                        class="input"
                        on:input:target=move |ev| {
                            let input_value = ev.target().value();
                            set_input_value(
                                "description",
                                input_value,
                                set_machine_form_data,
                                machine_form_data,
                                set_show_turn_off_port,
                            );
                        }
                        prop:value=move || {
                            machine_form_data.get().description.clone().unwrap_or_default()
                        }
                    />
                    <ErrorDisplay erros=erros key="description" />
                </div>

                <div class="field">
                    <div class="field-header">
                        <label>"Port forwards"</label>
                        <button
                            type="button"
                            class="btn btn-soft btn-sm"
                            on:click=move |_| {
                                set_port_forwards
                                    .update(|pfs| {
                                        pfs.push(PortForward {
                                            name: None,
                                            local_port: 0,
                                            target_port: 0,
                                        });
                                    });
                            }
                        >
                            "+ Add port"
                        </button>
                    </div>
                    <p class="field-help">
                        "Expose local TCP ports that should forward to the machine once it wakes."
                    </p>
                    <Show
                        when=move || !port_forwards.get().is_empty()
                        fallback=|| {
                            view! { <p class="field-empty">"No port forwards configured."</p> }
                        }
                    >
                        <div class="port-forward-list">
                            <For
                                each=move || {
                                    port_forwards
                                        .get()
                                        .into_iter()
                                        .enumerate()
                                        .collect::<Vec<(usize, PortForward)>>()
                                }
                                key=|(idx, _)| *idx
                                children=move |(idx, _)| {
                                    let row = idx + 1;
                                    let name_id = format!("pf-name-{}", row);
                                    let local_id = format!("pf-local-{}", row);
                                    let target_id = format!("pf-target-{}", row);

                                    view! {
                                        <div class="port-forward-item">
                                            <div class="port-forward-item__header">
                                                <span class="port-forward-item__title">
                                                    {format!("Forward {}", row)}
                                                </span>
                                                <button
                                                    type="button"
                                                    class="btn btn-ghost btn-sm port-forward-item__remove"
                                                    on:click=move |_| {
                                                        set_port_forwards
                                                            .update(|pfs| {
                                                                if idx < pfs.len() {
                                                                    pfs.remove(idx);
                                                                }
                                                            });
                                                    }
                                                >
                                                    "Remove"
                                                </button>
                                            </div>
                                            <div class="port-forward-item__grid">
                                                <div class="field">
                                                    <label for=name_id
                                                        .clone()>{format!("Service name {}", row)}</label>
                                                    <input
                                                        class="input"
                                                        id=name_id
                                                        placeholder="Service name"
                                                        prop:value=move || {
                                                            port_forwards
                                                                .get()
                                                                .get(idx)
                                                                .and_then(|pf| pf.name.clone())
                                                                .unwrap_or_default()
                                                        }
                                                        on:input=move |ev| {
                                                            let target = ev.target().unwrap();
                                                            let input: HtmlInputElement = target.dyn_into().unwrap();
                                                            let value = input.value();
                                                            let trimmed = value.trim().is_empty();
                                                            set_port_forwards
                                                                .update(|pfs| {
                                                                    if let Some(pf) = pfs.get_mut(idx) {
                                                                        pf.name = if trimmed { None } else { Some(value.clone()) };
                                                                    }
                                                                });
                                                        }
                                                    />
                                                </div>
                                                <div class="field">
                                                    <label for=local_id
                                                        .clone()>{format!("Local port {}", row)}</label>
                                                    <input
                                                        class="input"
                                                        id=local_id
                                                        placeholder="Local port"
                                                        type="number"
                                                        min="1"
                                                        max="65535"
                                                        prop:value=move || {
                                                            port_forwards
                                                                .get()
                                                                .get(idx)
                                                                .map(|pf| pf.local_port.to_string())
                                                                .unwrap_or_default()
                                                        }
                                                        on:input=move |ev| {
                                                            let target = ev.target().unwrap();
                                                            let input: HtmlInputElement = target.dyn_into().unwrap();
                                                            let parsed = input.value().parse::<u16>().unwrap_or(0);
                                                            set_port_forwards
                                                                .update(|pfs| {
                                                                    if let Some(pf) = pfs.get_mut(idx) {
                                                                        pf.local_port = parsed;
                                                                    }
                                                                });
                                                        }
                                                    />
                                                </div>
                                                <div class="field">
                                                    <label for=target_id
                                                        .clone()>{format!("Target port {}", row)}</label>
                                                    <input
                                                        class="input"
                                                        id=target_id
                                                        placeholder="Target port"
                                                        type="number"
                                                        min="1"
                                                        max="65535"
                                                        prop:value=move || {
                                                            port_forwards
                                                                .get()
                                                                .get(idx)
                                                                .map(|pf| pf.target_port.to_string())
                                                                .unwrap_or_default()
                                                        }
                                                        on:input=move |ev| {
                                                            let target = ev.target().unwrap();
                                                            let input: HtmlInputElement = target.dyn_into().unwrap();
                                                            let parsed = input.value().parse::<u16>().unwrap_or(0);
                                                            set_port_forwards
                                                                .update(|pfs| {
                                                                    if let Some(pf) = pfs.get_mut(idx) {
                                                                        pf.target_port = parsed;
                                                                    }
                                                                });
                                                        }
                                                    />
                                                </div>
                                            </div>
                                        </div>
                                    }
                                }
                            />
                        </div>
                    </Show>
                </div>

                <div class="field field-toggle">
                    <input
                        type="checkbox"
                        id="can_be_turned_off"
                        name="can_be_turned_off"
                        class="checkbox"
                        prop:checked=move || machine_form_data.get().can_be_turned_off
                        on:input:target=move |ev| {
                            let input_value = if ev.target().checked() { "on" } else { "off" }
                                .to_string();
                            set_input_value(
                                "can_be_turned_off",
                                input_value,
                                set_machine_form_data,
                                machine_form_data,
                                set_show_turn_off_port,
                            );
                        }
                    />
                    <div class="field-toggle__content">
                        <label for="can_be_turned_off">"Allow remote turn off"</label>
                        <p class="field-help">
                            "Requires the machine to expose a shutdown endpoint."
                        </p>
                    </div>
                </div>

                <Show when=move || show_turn_off_port.get() fallback=|| view! { <></> }>
                    {move || {
                        view! {
                            <div class="field">
                                <label for="turn_off_port">"Turn off port (optional)"</label>
                                <input
                                    type="number"
                                    id="turn_off_port"
                                    name="turn_off_port"
                                    class="input"
                                    min="1"
                                    max="65535"
                                    on:input:target=move |ev| {
                                        let input_value = ev.target().value();
                                        set_input_value(
                                            "turn_off_port",
                                            input_value,
                                            set_machine_form_data,
                                            machine_form_data,
                                            set_show_turn_off_port,
                                        );
                                    }
                                    prop:value=move || {
                                        machine_form_data
                                            .get()
                                            .turn_off_port
                                            .map(|port| port.to_string())
                                            .unwrap_or_default()
                                    }
                                />
                                <ErrorDisplay erros=erros key="turn_off_port" />
                            </div>
                        }
                    }}
                </Show>
                <div class="form-footer">
                    <button type="submit" class="btn btn-primary" disabled=move || loading.get()>
                        {move || {
                            if loading.get() { "Adding machine…" } else { "Add machine" }
                        }}
                    </button>
                </div>
            </form>
        </section>
    }
}
