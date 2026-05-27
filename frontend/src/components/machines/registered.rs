use leptos::prelude::*;
use std::collections::HashMap;

use web_sys::window;

use leptos::leptos_dom::logging::console_log;

use crate::api::{delete_machine, turn_off_machine, wake_machine};
use crate::models::Machine;

#[component]
pub fn RegisteredMachines(
    machines: ReadSignal<Vec<Machine>>,
    status_machine: ReadSignal<HashMap<String, bool>>,
    set_registred_machines: WriteSignal<Vec<Machine>>,
) -> impl IntoView {
    let (wake_in_progress, set_wake_in_progress) = signal::<Option<String>>(None);
    let (turn_off_in_progress, set_turn_off_in_progress) = signal::<Option<String>>(None);

    let on_delete = move |mac_to_delete: String| {
        leptos::task::spawn_local(async move {
            // Call the API to delete the machine
            if let Err(err) = delete_machine(&mac_to_delete).await {
                console_log(&format!(
                    "Error deleting machine {}: {}",
                    mac_to_delete, err
                ));
                window()
                    .unwrap()
                    .alert_with_message(&format!("Error deleting machine: {}", err))
                    .unwrap();
                return;
            }

            // Remove the machine from the local state
            let current_machines = machines.get();
            let filtered_machines: Vec<Machine> = current_machines
                .into_iter()
                .filter(|m| m.mac != mac_to_delete)
                .collect();

            set_registred_machines.set(filtered_machines);
            console_log(&format!("Successfully deleted machine: {}", mac_to_delete));
        });
    };

    view! {
        <section class="card table-card">
            <div class="card-header">
                <div>
                    <h2 class="card-title">"Registered machines"</h2>
                    <p class="card-subtitle">
                        {move || {
                            let count = machines.get().len();
                            if count == 0 {
                                "No machines registered yet.".to_string()
                            } else if count == 1 {
                                "1 machine online or ready".to_string()
                            } else {
                                format!("{} machines online or ready", count)
                            }
                        }}
                    </p>
                </div>
            </div>
            <div class="table-container">
                <table class="table">
                    <thead>
                        <tr>
                            <th>"Name"</th>
                            <th class="hide-mobile">"MAC"</th>
                            <th class="hide-mobile">"IP"</th>
                            <th class="hide-mobile">"Description"</th>
                            <th class="hide-mobile">"Port"</th>
                            <th class="hide-mobile">"Turn Off"</th>
                            <th>"Status"</th>
                            <th class="hide-mobile">"Forwards"</th>
                            <th>"Actions"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <Show
                            when=move || !machines.get().is_empty()
                            fallback=|| {
                                view! {
                                    <tr>
                                        <td colspan=9 class="table-empty">
                                            "No machines yet. Use the form below to add one."
                                        </td>
                                    </tr>
                                }
                            }
                        >
                            <For
                                each=move || machines.get()
                                key=|machine| machine.mac.clone()
                                children=move |machine| {
                                    let mac_href = machine.mac.clone();
                                    let mac_display = mac_href.clone();
                                    let ip_display = machine.ip.clone();
                                    let description_display = machine
                                        .description
                                        .clone()
                                        .unwrap_or_else(|| "-".to_string());
                                    let status_mac = mac_href.clone();
                                    let wake_mac_disabled = mac_href.clone();
                                    let wake_mac_click = mac_href.clone();
                                    let wake_mac_task = mac_href.clone();
                                    let turn_off_mac_disabled = mac_href.clone();
                                    let turn_off_mac_click = mac_href.clone();
                                    let turn_off_mac_task = mac_href.clone();
                                    let turn_off_mac_label = mac_href.clone();
                                    let delete_mac = mac_href.clone();
                                    let name_link = machine.name.clone();
                                    let name_for_wake = machine.name.clone();
                                    let name_for_turnoff = machine.name.clone();
                                    let name_for_confirm = machine.name.clone();
                                    let set_wake_in_progress_btn = set_wake_in_progress;
                                    let wake_in_progress_for_disable = wake_in_progress;
                                    let wake_in_progress_for_click = wake_in_progress;
                                    let set_turn_off_in_progress_btn = set_turn_off_in_progress;
                                    let turn_off_in_progress_for_disable = turn_off_in_progress;
                                    let turn_off_in_progress_for_click = turn_off_in_progress;
                                    let turn_off_in_progress_for_label = turn_off_in_progress;
                                    let can_turn_off_machine = machine.can_be_turned_off
                                        && machine.turn_off_port.is_some();
                                    let turn_off_port_text = machine
                                        .turn_off_port
                                        .map(|port| port.to_string())
                                        .unwrap_or_else(|| "-".to_string());
                                    let can_turn_off_text = if machine.can_be_turned_off {
                                        "Yes".to_string()
                                    } else {
                                        "No".to_string()
                                    };
                                    let port_forwards_text = if machine.port_forwards.is_empty() {
                                        "-".to_string()
                                    } else {
                                        machine
                                            .port_forwards
                                            .iter()
                                            .map(|pf| {
                                                let pf_name = pf
                                                    .name
                                                    .clone()
                                                    .unwrap_or_else(|| "-".to_string());
                                                format!(
                                                    "{} → {} ({})",
                                                    pf.local_port,
                                                    pf.target_port,
                                                    pf_name,
                                                )
                                            })
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    };
                                    let mobile_port_forward_labels: Vec<String> = if machine
                                        .port_forwards
                                        .is_empty()
                                    {
                                        vec!["-".to_string()]
                                    } else {
                                        machine
                                            .port_forwards
                                            .iter()
                                            .map(|pf| {
                                                let pf_name = pf
                                                    .name
                                                    .clone()
                                                    .unwrap_or_else(|| "-".to_string());
                                                format!(
                                                    "{} → {} ({})",
                                                    pf.local_port,
                                                    pf.target_port,
                                                    pf_name,
                                                )
                                            })
                                            .collect::<Vec<_>>()
                                    };

                                    view! {
                                        <tr>
                                            <td>
                                                <a
                                                    class="text-link"
                                                    href=format!("/machines/{}", mac_href.clone())
                                                >
                                                    {name_link}
                                                </a>
                                            </td>
                                            <td class="hide-mobile">
                                                <code>{mac_display.clone()}</code>
                                            </td>
                                            <td class="hide-mobile">
                                                <code>{ip_display.clone()}</code>
                                            </td>
                                            <td class="hide-mobile">{description_display.clone()}</td>
                                            <td class="hide-mobile">
                                                <span class="font-mono text-xs sm:text-sm">
                                                    {move || turn_off_port_text.clone()}
                                                </span>
                                            </td>
                                            <td class="hide-mobile">
                                                <span class="text-xs sm:text-sm">
                                                    {move || can_turn_off_text.clone()}
                                                </span>
                                            </td>
                                            <td>
                                                {move || {
                                                    let key = status_mac.clone();
                                                    let is_online = status_machine
                                                        .get()
                                                        .get(&key)
                                                        .cloned()
                                                        .unwrap_or(false);
                                                    if is_online {
                                                        view! {
                                                            <span class="status-pill status-pill--online">
                                                                "Online"
                                                            </span>
                                                        }
                                                    } else {
                                                        view! {
                                                            <span class="status-pill status-pill--offline">
                                                                "Offline"
                                                            </span>
                                                        }
                                                    }
                                                }}
                                            </td>
                                            <td class="hide-mobile">
                                                <span class="font-mono text-xs sm:text-sm">
                                                    {move || port_forwards_text.clone()}
                                                </span>
                                            </td>
                                            <td class="table-actions">
                                                <button
                                                    class="btn-icon btn-icon--positive"
                                                    title="Wake machine"
                                                    disabled=move || {
                                                        wake_in_progress_for_disable
                                                            .get()
                                                            .as_ref()
                                                            .map(|current| current == &wake_mac_disabled)
                                                            .unwrap_or(false)
                                                    }
                                                    on:click=move |_| {
                                                        if wake_in_progress_for_click
                                                            .get()
                                                            .as_ref()
                                                            .map(|current| current == &wake_mac_click)
                                                            .unwrap_or(false)
                                                        {
                                                            return;
                                                        }
                                                        set_wake_in_progress_btn.set(Some(wake_mac_task.clone()));
                                                        let set_wake_after = set_wake_in_progress_btn;
                                                        let mac_for_request = wake_mac_task.clone();
                                                        let name_for_alert = name_for_wake.clone();
                                                        leptos::task::spawn_local(async move {
                                                            match wake_machine(&mac_for_request).await {
                                                                Ok(message) => {
                                                                    if let Some(win) = window() {
                                                                        let _ = win.alert_with_message(&message);
                                                                    }
                                                                    console_log(
                                                                        &format!(
                                                                            "Wake request sent for {} ({})",
                                                                            name_for_alert,
                                                                            mac_for_request,
                                                                        ),
                                                                    );
                                                                }
                                                                Err(err) => {
                                                                    if let Some(win) = window() {
                                                                        let _ = win
                                                                            .alert_with_message(
                                                                                &format!("Failed to wake machine: {}", err),
                                                                            );
                                                                    }
                                                                    console_log(
                                                                        &format!(
                                                                            "Failed to wake {} ({}): {}",
                                                                            name_for_alert,
                                                                            mac_for_request,
                                                                            err,
                                                                        ),
                                                                    );
                                                                }
                                                            }
                                                            set_wake_after.set(None);
                                                        });
                                                    }
                                                >
                                                    "⚡"
                                                </button>
                                                <button
                                                    class="btn-icon"
                                                    title="Turn off machine"
                                                    disabled=move || {
                                                        !can_turn_off_machine
                                                            || turn_off_in_progress_for_disable
                                                                .get()
                                                                .as_ref()
                                                                .map(|current| current == &turn_off_mac_disabled)
                                                                .unwrap_or(false)
                                                    }
                                                    on:click=move |_| {
                                                        if !can_turn_off_machine {
                                                            if let Some(win) = window() {
                                                                let _ = win
                                                                    .alert_with_message(
                                                                        "Enable remote turn-off with a valid port before triggering this action.",
                                                                    );
                                                            }
                                                            return;
                                                        }
                                                        if turn_off_in_progress_for_click
                                                            .get()
                                                            .as_ref()
                                                            .map(|current| current == &turn_off_mac_click)
                                                            .unwrap_or(false)
                                                        {
                                                            return;
                                                        }
                                                        set_turn_off_in_progress_btn
                                                            .set(Some(turn_off_mac_task.clone()));
                                                        let set_turn_off_after = set_turn_off_in_progress_btn;
                                                        let mac_for_request = turn_off_mac_task.clone();
                                                        let name_for_alert = name_for_turnoff.clone();
                                                        leptos::task::spawn_local(async move {
                                                            match turn_off_machine(&mac_for_request).await {
                                                                Ok(message) => {
                                                                    if let Some(win) = window() {
                                                                        let _ = win.alert_with_message(&message);
                                                                    }
                                                                    console_log(
                                                                        &format!(
                                                                            "Turn-off request sent for {} ({})",
                                                                            name_for_alert,
                                                                            mac_for_request,
                                                                        ),
                                                                    );
                                                                }
                                                                Err(err) => {
                                                                    if let Some(win) = window() {
                                                                        let _ = win
                                                                            .alert_with_message(
                                                                                &format!("Failed to turn off machine: {}", err),
                                                                            );
                                                                    }
                                                                    console_log(
                                                                        &format!(
                                                                            "Failed to turn off {} ({}): {}",
                                                                            name_for_alert,
                                                                            mac_for_request,
                                                                            err,
                                                                        ),
                                                                    );
                                                                }
                                                            }
                                                            set_turn_off_after.set(None);
                                                        });
                                                    }
                                                >
                                                    {move || {
                                                        if turn_off_in_progress_for_label
                                                            .get()
                                                            .as_ref()
                                                            .map(|current| current == &turn_off_mac_label)
                                                            .unwrap_or(false)
                                                        {
                                                            "⏳"
                                                        } else {
                                                            "⏻"
                                                        }
                                                    }}
                                                </button>
                                                <button
                                                    class="btn-icon btn-icon--danger"
                                                    title="Delete machine"
                                                    on:click=move |_| {
                                                        if window()
                                                            .unwrap()
                                                            .confirm_with_message(
                                                                &format!(
                                                                    "Are you sure you want to delete machine {}?",
                                                                    name_for_confirm.clone(),
                                                                ),
                                                            )
                                                            .unwrap_or(false)
                                                        {
                                                            on_delete(delete_mac.clone());
                                                        }
                                                    }
                                                >
                                                    "🗑"
                                                </button>
                                                <div class="mobile-port-forwards show-mobile">
                                                    <span class="mobile-port-forwards__title">
                                                        "Port forwards"
                                                    </span>
                                                    <div class="mobile-port-forwards__list">
                                                        {mobile_port_forward_labels
                                                            .iter()
                                                            .cloned()
                                                            .map(|label| {
                                                                view! {
                                                                    <span class="mobile-port-forwards__chip">{label}</span>
                                                                }
                                                            })
                                                            .collect::<Vec<_>>()}
                                                    </div>
                                                </div>
                                            </td>
                                        </tr>
                                    }
                                }
                            />
                        </Show>
                    </tbody>
                </table>
            </div>
        </section>
    }
}
