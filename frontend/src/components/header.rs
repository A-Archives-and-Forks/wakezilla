use leptos::prelude::*;

use web_sys::window;

use leptos::leptos_dom::logging::console_log;

use web_sys::{SubmitEvent, console};

use crate::api::{fetch_interfaces, fetch_scan_network};
use crate::models::{DiscoveredDevice, Machine, NetworkInterface, PortForward};

#[component]
pub fn Header(
    set_machine: WriteSignal<Machine>,
    registred_machines: ReadSignal<Vec<Machine>>,
) -> impl IntoView {
    let (discovered_devices, set_discovered_devices) = signal::<Vec<DiscoveredDevice>>(vec![]);
    let (interfaces, set_interfaces) = signal::<Vec<NetworkInterface>>(vec![]);
    let (interface, set_interface) = signal::<String>("".to_string());
    let (loading, set_loading) = signal(false);

    // Load initial data
    Effect::new(move || {
        leptos::task::spawn_local(async move {
            if let Ok(cats) = fetch_interfaces().await {
                set_interfaces.set(cats);
            }
        });
    });

    fn handle_interface_change(value: String, set_interface: WriteSignal<String>) {
        let log_mesasge = format!("Selected interface: {}", value);
        console_log(&log_mesasge);
        set_interface.set(value);
    }

    let on_submit = move |ev: SubmitEvent| {
        let set_loading = set_loading;
        set_loading.set(true);
        set_discovered_devices.set(vec![]);
        // stop the page from reloading!
        ev.prevent_default();
        console::log_1(&format!("Form submitted with value: {}", interface.get()).into());
        leptos::task::spawn_local(async move {
            fetch_scan_network(interface.get())
                .await
                .map(|devices| {
                    console::log_1(&format!("Discovered devices: {:?}", devices).into());
                    // does not diplay the machine if it's already registred
                    let registred_machines = registred_machines.get();
                    let devices: Vec<DiscoveredDevice> = devices
                        .into_iter()
                        .filter(|device| {
                            !registred_machines
                                .iter()
                                .any(|machine| machine.mac == device.mac)
                        })
                        .collect();

                    set_discovered_devices.set(devices);
                })
                .unwrap_or_else(|err| {
                    window()
                        .unwrap()
                        .alert_with_message("Error scanning network, check the logs in the backend")
                        .unwrap();
                    console::log_1(&format!("Error scanning network: {}", err).into());
                });

            set_loading.set(false);
        });
    };

    fn handle_add_machine(
        device: DiscoveredDevice,
        set_machine: WriteSignal<Machine>,
        set_discovered_devices: WriteSignal<Vec<DiscoveredDevice>>,
    ) {
        let new_machine = Machine {
            name: device.hostname.clone().unwrap_or_default(),
            mac: device.mac.clone(),
            ip: device.ip.clone(),
            description: None,
            turn_off_port: Some(3000),
            can_be_turned_off: false,
            inactivity_period: 60,
            port_forwards: vec![PortForward {
                name: None,
                local_port: 0,
                target_port: 0,
            }],
        };
        set_machine.set(new_machine);
        set_discovered_devices.set(vec![]);
    }

    view! {
        <div class="section-stack">
            <div class="card scan-card">
                <header class="card-header card-header--with-logo">
                    <div class="card-header__text">
                        <div style="display: flex; align-items: center; gap: 10px;">
                            <img
                                src="/images/wakezilla.png"
                                alt="Wakezilla logo"
                                class="card-header__logo"
                            />
                            <h1 class="">"Wakezilla"</h1>
                        </div>
                        <p class="card-subtitle">
                            "Wake, manage, and forward to your registered machines."
                        </p>
                    </div>
                </header>
                <form on:submit=on_submit class="scan-grid">
                    <select
                        id="interface-select"
                        class="input"
                        on:change:target=move |ev| {
                            handle_interface_change(ev.target().value(), set_interface);
                        }
                        prop:value=move || interface.get().to_string()
                    >
                        <option value="">"Auto-detect interface"</option>
                        {move || {
                            interfaces
                                .get()
                                .iter()
                                .map(|iface| {
                                    view! {
                                        <option value=iface
                                            .name
                                            .clone()>
                                            {format!("{} · {} ({})", iface.name, iface.ip, iface.mac)}
                                        </option>
                                    }
                                })
                                .collect::<Vec<_>>()
                        }}
                    </select>
                    <button id="scan-btn" class="btn btn-primary" disabled=move || loading.get()>
                        {move || { if loading.get() { "Scanning…" } else { "Scan network" } }}
                    </button>
                </form>
            </div>

            <Show when=move || { !discovered_devices.get().is_empty() } fallback=|| view! { <></> }>
                <div class="card table-card" id="scan-results-container">
                    <div class="card-header">
                        <h3 class="card-title">"Discovered devices"</h3>
                        <p class="card-subtitle">
                            "Tap a device to pre-fill the create form below."
                        </p>
                    </div>
                    <div class="table-container">
                        <table class="table" id="scan-results-table">
                            <thead>
                                <tr>
                                    <th>"IP address"</th>
                                    <th>"Hostname"</th>
                                    <th>"MAC address"</th>
                                    <th>"Action"</th>
                                </tr>
                            </thead>
                            <tbody>
                                <For
                                    each=move || discovered_devices.get()
                                    key=|device| device.ip.clone()
                                    children=move |device| {
                                        view! {
                                            <tr>
                                                <td attr:data-label="IP address">{device.ip.clone()}</td>
                                                <td attr:data-label="Hostname">
                                                    {device
                                                        .hostname
                                                        .clone()
                                                        .unwrap_or_else(|| "N/A".to_string())}
                                                </td>
                                                <td attr:data-label="MAC address">{device.mac.clone()}</td>
                                                <td attr:data-label="Action" class="table-actions">
                                                    <button
                                                        class="btn-icon btn-icon--positive"
                                                        title="Use this device"
                                                        on:click=move |_| {
                                                            handle_add_machine(
                                                                device.clone(),
                                                                set_machine,
                                                                set_discovered_devices,
                                                            );
                                                        }
                                                    >
                                                        "＋"
                                                    </button>
                                                </td>
                                            </tr>
                                        }
                                    }
                                />
                            </tbody>
                        </table>
                    </div>
                </div>
            </Show>
        </div>
    }
}
