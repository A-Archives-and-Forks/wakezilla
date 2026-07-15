use leptos::prelude::*;

use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;
use web_sys::window;

use leptos_router::hooks::use_params_map;

use web_sys::SubmitEvent;

use crate::api::{
    get_details_machine, get_shutdown_setup, rotate_shutdown_key, turn_off_machine,
    verify_shutdown_setup, wake_machine,
};
use crate::models::{
    Machine, PortForward, ShutdownSetup, ShutdownSetupStatus, UpdateMachinePayload,
};

use crate::api::get_access_history;
use crate::models::AccessHistory;
use chrono::{DateTime, Datelike, TimeZone, Utc};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(inline_js = r#"
export function render_usage_chart(canvas_id, labels_json, datasets_json) {
    if (typeof window.Chart === 'undefined') { return; }
    const el = document.getElementById(canvas_id);
    if (!el) { return; }
    const labels = JSON.parse(labels_json);
    const datasets = JSON.parse(datasets_json);
    const palette = ['#2563eb','#16a34a','#dc2626','#d97706','#7c3aed','#0891b2','#db2777'];
    datasets.forEach((d, i) => {
        d.backgroundColor = palette[i % palette.length];
        d.borderColor = palette[i % palette.length];
    });
    if (el._chart) { el._chart.destroy(); }
    el._chart = new window.Chart(el, {
        type: 'bar',
        data: { labels: labels, datasets: datasets },
        options: {
            responsive: true,
            scales: { y: { beginAtZero: true, ticks: { precision: 0 } } },
            plugins: { legend: { position: 'bottom' } }
        }
    });
}
export async function copy_to_clipboard(value) {
    if (navigator.clipboard && window.isSecureContext) {
        await navigator.clipboard.writeText(value);
        return;
    }

    const textarea = document.createElement('textarea');
    textarea.value = value;
    textarea.setAttribute('readonly', '');
    textarea.style.position = 'fixed';
    textarea.style.opacity = '0';
    document.body.appendChild(textarea);
    textarea.select();

    try {
        if (!document.execCommand('copy')) {
            throw new Error('copy command was rejected');
        }
    } finally {
        document.body.removeChild(textarea);
    }
}
"#)]
extern "C" {
    fn render_usage_chart(canvas_id: &str, labels_json: &str, datasets_json: &str);
    fn copy_to_clipboard(value: &str) -> js_sys::Promise;
}

const UNIX_INSTALL_COMMAND: &str = "curl -fsSL https://wakezilla.dev/install.sh | sh";
const WINDOWS_INSTALL_COMMAND: &str = "irm https://wakezilla.dev/install.ps1 | iex";

#[derive(Clone, Copy, PartialEq, Eq)]
enum CopyFeedback {
    Idle,
    Copying,
    Copied,
    Failed,
}

impl CopyFeedback {
    fn label(self) -> &'static str {
        match self {
            Self::Idle => "Copy command",
            Self::Copying => "Copying...",
            Self::Copied => "Copied!",
            Self::Failed => "Copy failed",
        }
    }

    fn button_class(self) -> &'static str {
        match self {
            Self::Idle | Self::Copying => "btn btn-soft btn-sm",
            Self::Copied => "btn btn-success btn-sm",
            Self::Failed => "btn btn-danger btn-sm",
        }
    }
}

#[component]
fn SetupCommandStep(label: &'static str, command: String) -> impl IntoView {
    let command_to_copy = command.clone();
    let copy_label = format!("Copy {label}");
    let (copy_feedback, set_copy_feedback) = signal(CopyFeedback::Idle);

    let copy_command = move |_| {
        let command = command_to_copy.clone();
        set_copy_feedback.set(CopyFeedback::Copying);

        leptos::task::spawn_local(async move {
            let feedback = if JsFuture::from(copy_to_clipboard(&command)).await.is_ok() {
                CopyFeedback::Copied
            } else {
                CopyFeedback::Failed
            };
            set_copy_feedback.set(feedback);
            gloo_timers::future::TimeoutFuture::new(2_000).await;
            set_copy_feedback.set(CopyFeedback::Idle);
        });
    };

    view! {
        <div class="setup-command">
            <div class="field-header">
                <strong>{label}</strong>
                <button
                    type="button"
                    class=move || copy_feedback.get().button_class()
                    aria-label=copy_label
                    disabled=move || copy_feedback.get() != CopyFeedback::Idle
                    on:click=copy_command
                >
                    {move || copy_feedback.get().label()}
                </button>
                <span class="sr-only" aria-live="polite">
                    {move || match copy_feedback.get() {
                        CopyFeedback::Copied => "Command copied to clipboard",
                        CopyFeedback::Failed => "Could not copy command",
                        CopyFeedback::Idle | CopyFeedback::Copying => "",
                    }}
                </span>
            </div>
            <pre class="code-block">{command}</pre>
        </div>
    }
}

fn shutdown_setup_message(status: ShutdownSetupStatus) -> &'static str {
    match status {
        ShutdownSetupStatus::Disabled => "Remote shutdown is disabled.",
        ShutdownSetupStatus::Legacy => {
            "Remote shutdown is using legacy unsigned requests. Secure it to restrict access."
        }
        ShutdownSetupStatus::Pending => {
            "Run this command on the client machine. This page will verify it automatically."
        }
        ShutdownSetupStatus::Verified => {
            "The client is paired and shutdown requests are authenticated."
        }
        ShutdownSetupStatus::Unreachable => {
            "Waiting for the client to become reachable. The setup command will remain available."
        }
        ShutdownSetupStatus::KeyMismatch => {
            "The client responded, but its key does not match. Run the setup command again."
        }
    }
}

fn shutdown_control_is_visible(status: Option<ShutdownSetupStatus>) -> bool {
    matches!(
        status,
        Some(ShutdownSetupStatus::Legacy | ShutdownSetupStatus::Verified)
    )
}

const fn raw_machine_data_is_visible() -> bool {
    cfg!(debug_assertions)
}

async fn monitor_shutdown_setup(
    mac: String,
    set_shutdown_setup: WriteSignal<Option<ShutdownSetup>>,
) {
    const INITIAL_POLL_DELAY_MS: u32 = 3_000;

    let mut setup = match get_shutdown_setup(&mac).await {
        Ok(setup) => setup,
        Err(_) => return,
    };
    set_shutdown_setup.set(Some(setup.clone()));
    let mut poll_delay_ms = INITIAL_POLL_DELAY_MS;

    while matches!(
        setup.status,
        ShutdownSetupStatus::Pending
            | ShutdownSetupStatus::Unreachable
            | ShutdownSetupStatus::KeyMismatch
    ) {
        gloo_timers::future::TimeoutFuture::new(poll_delay_ms).await;
        match verify_shutdown_setup(&mac).await {
            Ok(next) => {
                setup = next;
                set_shutdown_setup.set(Some(setup.clone()));
                poll_delay_ms = next_shutdown_poll_delay(poll_delay_ms, true);
            }
            Err(_) => {
                setup.status = ShutdownSetupStatus::Unreachable;
                set_shutdown_setup.set(Some(setup.clone()));
                poll_delay_ms = next_shutdown_poll_delay(poll_delay_ms, false);
            }
        }
    }
}

fn next_shutdown_poll_delay(current_ms: u32, request_succeeded: bool) -> u32 {
    const INITIAL_POLL_DELAY_MS: u32 = 3_000;
    const MAX_POLL_DELAY_MS: u32 = 30_000;

    if request_succeeded {
        INITIAL_POLL_DELAY_MS
    } else {
        current_ms.saturating_mul(2).min(MAX_POLL_DELAY_MS)
    }
}

fn rotation_failure_message(message: &str) -> String {
    format!("Failed to rotate shutdown key: {message}")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AccessHistoryBucket {
    Day,
    Hour,
    Week,
}

impl AccessHistoryBucket {
    fn label(self, dt: DateTime<Utc>) -> String {
        match self {
            Self::Day => dt.format("%Y-%m-%d").to_string(),
            Self::Hour => dt.format("%Y-%m-%d %H:00").to_string(),
            Self::Week => {
                let week = dt.iso_week();
                format!("{}-W{:02}", week.year(), week.week())
            }
        }
    }
}

// Buckets timestamps by the selected period. Returns (sorted bucket labels,
// per-service datasets as JSON values with counts aligned to labels).
fn bucket_history(
    history: &AccessHistory,
    bucket: AccessHistoryBucket,
) -> (Vec<String>, Vec<serde_json::Value>) {
    use std::collections::{BTreeSet, HashMap};

    let bucket_of = |ts: i64| -> String {
        let dt: DateTime<Utc> = Utc
            .timestamp_millis_opt(ts)
            .single()
            .unwrap_or_else(Utc::now);
        bucket.label(dt)
    };

    let mut all_buckets: BTreeSet<String> = BTreeSet::new();
    let mut per_service: Vec<(String, HashMap<String, u32>)> = Vec::new();

    for svc in &history.services {
        let label = svc
            .name
            .clone()
            .unwrap_or_else(|| format!("port {}", svc.local_port));
        let mut counts: HashMap<String, u32> = HashMap::new();
        for &ts in &svc.timestamps {
            let bucket = bucket_of(ts);
            all_buckets.insert(bucket.clone());
            *counts.entry(bucket).or_insert(0) += 1;
        }
        per_service.push((label, counts));
    }

    let labels: Vec<String> = all_buckets.into_iter().collect();
    let datasets: Vec<serde_json::Value> = per_service
        .into_iter()
        .map(|(label, counts)| {
            let data: Vec<u32> = labels
                .iter()
                .map(|d| *counts.get(d).unwrap_or(&0))
                .collect();
            serde_json::json!({ "label": label, "data": data })
        })
        .collect();

    (labels, datasets)
}

#[component]
pub fn MachineDetailPage() -> impl IntoView {
    let params = use_params_map();
    let mac = move || params.read().get("mac").unwrap_or_default();
    let (loading, set_loading) = signal(false);
    let (machine_details, set_machine_details) = signal::<Machine>(Machine::default());
    let (shutdown_setup, set_shutdown_setup) = signal::<Option<ShutdownSetup>>(None);
    let (setup_loading, set_setup_loading) = signal(false);
    let (shutdown_setup_refresh, set_shutdown_setup_refresh) = signal(0u32);

    // Load initial machine details
    Effect::new(move || {
        leptos::task::spawn_local(async move {
            if let Ok(cats) = get_details_machine(&mac()).await {
                set_machine_details.set(cats);
            }
        });
    });

    Effect::new(move || {
        let mac = mac();
        shutdown_setup_refresh.get();
        leptos::task::spawn_local_scoped_with_cancellation(monitor_shutdown_setup(
            mac,
            set_shutdown_setup,
        ));
    });

    let (access_history, set_access_history) = signal::<Option<AccessHistory>>(None);
    let (history_refresh, set_history_refresh) = signal(0u32);
    let (history_loading, set_history_loading) = signal(false);

    Effect::new(move || {
        let mac_val = mac();
        history_refresh.get(); // re-run when the refresh button is pressed
        set_history_loading.set(true);
        leptos::task::spawn_local(async move {
            if let Ok(h) = get_access_history(&mac_val).await {
                set_access_history.set(Some(h));
            }
            set_history_loading.set(false);
        });
    });

    let (history_bucket, set_history_bucket) = signal(AccessHistoryBucket::Day);

    Effect::new(move || {
        if let Some(history) = access_history.get() {
            let (labels, datasets) = bucket_history(&history, history_bucket.get());
            let labels_json = serde_json::to_string(&labels).unwrap_or_else(|_| "[]".into());
            let datasets_json = serde_json::to_string(&datasets).unwrap_or_else(|_| "[]".into());
            render_usage_chart("usage-chart", &labels_json, &datasets_json);
        }
    });

    // Form state
    let (name, set_name) = signal(String::new());
    let (ip, set_ip) = signal(String::new());
    let (description, set_description) = signal(String::new());
    let (turn_off_port, set_turn_off_port) = signal::<Option<u16>>(None);
    let (can_be_turned_off, set_can_be_turned_off) = signal(false);
    let (port_forwards, set_port_forwards) = signal::<Vec<PortForward>>(vec![]);
    let (inactivity_period, set_inactivity_period) = signal(60u32);
    let (turn_off_loading, set_turn_off_loading) = signal(false);
    let (turn_off_feedback, set_turn_off_feedback) = signal::<Option<(bool, String)>>(None);
    let (wake_loading, set_wake_loading) = signal(false);
    let (wake_feedback, set_wake_feedback) = signal::<Option<(bool, String)>>(None);

    let can_turn_off_machine = Memo::new(move |_| {
        let machine = machine_details.get();
        machine.can_be_turned_off && machine.turn_off_port.is_some()
    });

    // Update form fields when machine details load
    Effect::new(move || {
        let machine = machine_details.get();
        set_name.set(machine.name.clone());
        set_ip.set(machine.ip.clone());
        set_description.set(machine.description.clone().unwrap_or_default());
        set_turn_off_port.set(machine.turn_off_port); // This should now match the type
        set_can_be_turned_off.set(machine.can_be_turned_off);
        set_port_forwards.set(machine.port_forwards.clone());
        set_inactivity_period.set(machine.inactivity_period);
    });

    let update_machine = move |ev: SubmitEvent| {
        ev.prevent_default();
        set_loading.set(true);

        let updated_mac = mac();
        let updated_name = name.get();
        let updated_ip = ip.get();
        let updated_description = if description.get().trim().is_empty() {
            None
        } else {
            Some(description.get())
        };
        let updated_turn_off_port = if can_be_turned_off.get() {
            turn_off_port.get()
        } else {
            None
        };
        let updated_can_be_turned_off = can_be_turned_off.get();
        let updated_port_forwards = port_forwards.get();

        // Create updated machine object for local state refresh
        let updated_machine = Machine {
            name: updated_name,
            mac: updated_mac.clone(),
            ip: updated_ip,
            description: updated_description,
            turn_off_port: updated_turn_off_port,
            can_be_turned_off: updated_can_be_turned_off,
            inactivity_period: inactivity_period.get(),
            port_forwards: updated_port_forwards.clone(),
        };

        let payload = UpdateMachinePayload {
            mac: updated_machine.mac.clone(),
            ip: updated_machine.ip.clone(),
            name: updated_machine.name.clone(),
            description: updated_machine.description.clone(),
            turn_off_port: updated_machine.turn_off_port,
            can_be_turned_off: updated_machine.can_be_turned_off,
            inactivity_period: Some(updated_machine.inactivity_period),
            port_forwards: Some(updated_machine.port_forwards),
        };

        leptos::task::spawn_local(async move {
            match crate::api::update_machine(&updated_mac, &payload).await {
                Ok(_) => {
                    web_sys::console::log_1(&"Machine updated successfully".into());
                    // Reload the machine details to reflect changes
                    if let Ok(updated_details) = get_details_machine(&updated_mac).await {
                        set_machine_details.set(updated_details);
                    }
                    set_shutdown_setup_refresh.update(|refresh| *refresh += 1);
                    window()
                        .unwrap()
                        .alert_with_message("Machine updated successfully!")
                        .unwrap();
                }
                Err(e) => {
                    web_sys::console::log_1(&format!("Error updating machine: {}", e).into());
                    window()
                        .unwrap()
                        .alert_with_message(&format!("Error updating machine: {}", e))
                        .unwrap();
                }
            }
            set_loading.set(false);
        });
    };

    let trigger_turn_off = move |_| {
        if !can_turn_off_machine.get() || turn_off_loading.get() {
            return;
        }

        let mac_address = mac();
        set_turn_off_loading.set(true);
        set_turn_off_feedback.set(None);

        let set_turn_off_loading = set_turn_off_loading;
        let set_turn_off_feedback = set_turn_off_feedback;

        leptos::task::spawn_local(async move {
            match turn_off_machine(&mac_address).await {
                Ok(message) => {
                    set_turn_off_feedback.set(Some((true, message.clone())));
                    if let Some(window) = window() {
                        let _ = window.alert_with_message(&message);
                    }
                }
                Err(message) => {
                    set_turn_off_feedback.set(Some((false, message.clone())));
                    if let Some(window) = window() {
                        let _ = window.alert_with_message(&format!(
                            "Failed to turn off machine: {}",
                            message
                        ));
                    }
                }
            }
            set_turn_off_loading.set(false);
        });
    };

    let trigger_wake = move |_| {
        if wake_loading.get() {
            return;
        }

        let mac_address = mac();
        set_wake_loading.set(true);
        set_wake_feedback.set(None);

        let set_wake_loading = set_wake_loading;
        let set_wake_feedback = set_wake_feedback;

        leptos::task::spawn_local(async move {
            match wake_machine(&mac_address).await {
                Ok(message) => {
                    set_wake_feedback.set(Some((true, message.clone())));
                    if let Some(window) = window() {
                        let _ = window.alert_with_message(&message);
                    }
                }
                Err(message) => {
                    set_wake_feedback.set(Some((false, message.clone())));
                    if let Some(window) = window() {
                        let _ = window
                            .alert_with_message(&format!("Failed to wake machine: {}", message));
                    }
                }
            }
            set_wake_loading.set(false);
        });
    };

    let rotate_setup = move |_| {
        if setup_loading.get() {
            return;
        }
        let status = shutdown_setup.get().map(|setup| setup.status);
        if status == Some(ShutdownSetupStatus::Verified) {
            let confirmed = window()
                .and_then(|window| {
                    window
                        .confirm_with_message(
                            "Generate a new shutdown key? The current client will stop authenticating until the new command is run.",
                        )
                        .ok()
                })
                .unwrap_or(false);
            if !confirmed {
                return;
            }
        }

        let mac = mac();
        set_setup_loading.set(true);
        leptos::task::spawn_local(async move {
            match rotate_shutdown_key(&mac).await {
                Ok(setup) => {
                    set_shutdown_setup.set(Some(setup));
                    set_shutdown_setup_refresh.update(|refresh| *refresh += 1);
                }
                Err(message) => {
                    if let Some(window) = window() {
                        let _ = window.alert_with_message(&rotation_failure_message(&message));
                    }
                }
            }
            set_setup_loading.set(false);
        });
    };

    view! {
        <div class="page-stack">
            <a class="back-link" href="/">
                <span aria-hidden="true">"←"</span>
                <span>"Back to dashboard"</span>
            </a>

            <Show
                when=move || {
                    shutdown_setup
                        .get()
                        .map(|setup| setup.status != ShutdownSetupStatus::Disabled)
                        .unwrap_or(false)
                }
                fallback=|| view! { <></> }
            >
                <div class="card">
                    <header class="card-header">
                        <h3 class="card-title">
                            {move || {
                                let needs_setup = shutdown_setup
                                    .get()
                                    .map(|setup| {
                                        setup.unix_command.is_some()
                                            || setup.windows_command.is_some()
                                    })
                                    .unwrap_or(false);
                                if needs_setup {
                                    "Finish setting up your client server"
                                } else {
                                    "Secure remote shutdown"
                                }
                            }}
                        </h3>
                        <p class="card-subtitle">
                            {move || {
                                shutdown_setup
                                    .get()
                                    .map(|setup| shutdown_setup_message(setup.status))
                                    .unwrap_or_default()
                            }}
                        </p>
                    </header>

                    <Show
                        when=move || {
                            shutdown_setup
                                .get()
                                .map(|setup| {
                                    setup.unix_command.is_some()
                                        || setup.windows_command.is_some()
                                })
                                .unwrap_or(false)
                        }
                        fallback=|| view! { <></> }
                    >
                        <div class="setup-platform">
                            <h4 class="setup-platform__title">"Linux / macOS"</h4>
                            <SetupCommandStep
                                label="1. Install Wakezilla"
                                command=UNIX_INSTALL_COMMAND.to_string()
                            />
                            {move || {
                                shutdown_setup
                                    .get()
                                    .and_then(|setup| setup.unix_command)
                                    .map(|command| {
                                        view! {
                                            <SetupCommandStep
                                                label="2. Configure the client server"
                                                command=command
                                            />
                                        }
                                    })
                            }}
                        </div>

                        <div class="setup-platform">
                            <h4 class="setup-platform__title">
                                "Windows (Administrator terminal)"
                            </h4>
                            <SetupCommandStep
                                label="1. Install Wakezilla"
                                command=WINDOWS_INSTALL_COMMAND.to_string()
                            />
                            {move || {
                                shutdown_setup
                                    .get()
                                    .and_then(|setup| setup.windows_command)
                                    .map(|command| {
                                        view! {
                                            <SetupCommandStep
                                                label="2. Configure the client server"
                                                command=command
                                            />
                                        }
                                    })
                            }}
                        </div>
                    </Show>

                    <Show
                        when=move || {
                            shutdown_setup
                                .get()
                                .map(|setup| {
                                    matches!(
                                        setup.status,
                                        ShutdownSetupStatus::Legacy | ShutdownSetupStatus::Verified
                                    )
                                })
                                .unwrap_or(false)
                        }
                        fallback=|| view! { <></> }
                    >
                        <div class="actions-row">
                            <button
                                type="button"
                                class="btn btn-soft"
                                disabled=move || setup_loading.get()
                                on:click=rotate_setup
                            >
                                {move || {
                                    if setup_loading.get() {
                                        "Generating..."
                                    } else if shutdown_setup.get().map(|setup| setup.status)
                                        == Some(ShutdownSetupStatus::Legacy)
                                    {
                                        "Secure now"
                                    } else {
                                        "Reconfigure security"
                                    }
                                }}
                            </button>
                        </div>
                    </Show>
                </div>
            </Show>

            <div class="card">
                <header class="card-header">
                    <h2 class="card-title">
                        {move || {
                            let current_name = name.get();
                            if current_name.trim().is_empty() {
                                "Machine Overview".to_string()
                            } else {
                                current_name
                            }
                        }}
                    </h2>
                    <p class="card-subtitle">
                        {move || format!("MAC {}", machine_details.get().mac)}
                    </p>
                </header>

                <form on:submit=update_machine class="form-grid">
                    <div class="form-grid two-column">
                        <div class="field">
                            <label for="name">"Name"</label>
                            <input
                                type="text"
                                id="name"
                                name="name"
                                class="input"
                                required
                                value=move || name.get()
                                on:input=move |ev| {
                                    let target = ev.target().unwrap();
                                    let input: HtmlInputElement = target.dyn_into().unwrap();
                                    set_name.set(input.value());
                                }
                            />
                        </div>
                        <div class="field">
                            <label for="ip">"IP address"</label>
                            <input
                                type="text"
                                id="ip"
                                name="ip"
                                class="input"
                                required
                                value=move || ip.get()
                                on:input=move |ev| {
                                    let target = ev.target().unwrap();
                                    let input: HtmlInputElement = target.dyn_into().unwrap();
                                    set_ip.set(input.value());
                                }
                            />
                        </div>
                    </div>

                    <div class="field">
                        <label for="description">"Description"</label>
                        <input
                            type="text"
                            id="description"
                            name="description"
                            class="input"
                            value=move || description.get()
                            on:input=move |ev| {
                                let target = ev.target().unwrap();
                                let input: HtmlInputElement = target.dyn_into().unwrap();
                                set_description.set(input.value());
                            }
                        />
                        <p class="field-help">
                            "Optional label to help the team recognise this machine."
                        </p>
                    </div>

                    <div class="field field-toggle">
                        <input
                            type="checkbox"
                            id="can_be_turned_off"
                            name="can_be_turned_off"
                            class="checkbox"
                            checked=move || can_be_turned_off.get()
                            on:change=move |ev| {
                                let target = ev.target().unwrap();
                                let input: HtmlInputElement = target.dyn_into().unwrap();
                                set_can_be_turned_off.set(input.checked());
                            }
                        />
                        <div class="field-toggle__content">
                            <label for="can_be_turned_off">"Enable remote turn off"</label>
                            <p class="field-help">
                                "Requires an accessible shutdown endpoint on the machine."
                            </p>
                        </div>
                    </div>

                    <Show when=move || can_be_turned_off.get() fallback=|| view! { <></> }>
                        <div class="field">
                            <label for="turn_off_port">"Turn off port (optional)"</label>
                            <input
                                type="number"
                                id="turn_off_port"
                                name="turn_off_port"
                                class="input"
                                min="1"
                                max="65535"
                                value=move || {
                                    turn_off_port.get().map(|p| p.to_string()).unwrap_or_default()
                                }
                                on:input=move |ev| {
                                    let target = ev.target().unwrap();
                                    let input: HtmlInputElement = target.dyn_into().unwrap();
                                    let value = input.value();
                                    set_turn_off_port.set(value.parse().ok());
                                }
                            />
                            <p class="field-help">
                                "Port exposed by the machine to receive shutdown requests."
                            </p>
                        </div>
                    </Show>

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
                            "Start lightweight TCP tunnels when this machine is online."
                        </p>
                        <Show
                            when=move || !port_forwards.get().is_empty()
                            fallback=|| {
                                view! {
                                    <p class="field-empty">"No port forwards configured yet."</p>
                                }
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
                                    children=move |(idx, _port_forward)| {
                                        let row_number = idx + 1;
                                        let name_id = format!("pf-name-{}", row_number);
                                        let local_id = format!("pf-local-{}", row_number);
                                        let target_id = format!("pf-target-{}", row_number);
                                        let name_label = format!("Service name {}", row_number);
                                        let local_label = format!("Local port {}", row_number);
                                        let target_label = format!(
                                            "Forward to port {}",
                                            row_number,
                                        );
                                        let forward_label = format!("Forward {}", row_number);

                                        view! {
                                            <div class="port-forward-item">
                                                <div class="port-forward-item__header">
                                                    <span class="port-forward-item__title">
                                                        {forward_label}
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
                                                        <label for=name_id.clone()>{name_label.clone()}</label>
                                                        <input
                                                            class="input"
                                                            id=name_id
                                                            placeholder="Service name"
                                                            value=move || {
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
                                                        <label for=local_id.clone()>{local_label.clone()}</label>
                                                        <input
                                                            class="input"
                                                            id=local_id
                                                            placeholder="Local port"
                                                            type="number"
                                                            min="0"
                                                            max="65535"
                                                            value=move || {
                                                                port_forwards
                                                                    .get()
                                                                    .get(idx)
                                                                    .map(|pf| pf.local_port.to_string())
                                                                    .unwrap_or_default()
                                                            }
                                                            on:input=move |ev| {
                                                                let target = ev.target().unwrap();
                                                                let input: HtmlInputElement = target.dyn_into().unwrap();
                                                                let value = input.value();
                                                                let parsed = value.parse::<u16>().unwrap_or(0);
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
                                                        <label for=target_id.clone()>{target_label.clone()}</label>
                                                        <input
                                                            class="input"
                                                            id=target_id
                                                            placeholder="Target port"
                                                            type="number"
                                                            min="0"
                                                            max="65535"
                                                            value=move || {
                                                                port_forwards
                                                                    .get()
                                                                    .get(idx)
                                                                    .map(|pf| pf.target_port.to_string())
                                                                    .unwrap_or_default()
                                                            }
                                                            on:input=move |ev| {
                                                                let target = ev.target().unwrap();
                                                                let input: HtmlInputElement = target.dyn_into().unwrap();
                                                                let value = input.value();
                                                                let parsed = value.parse::<u16>().unwrap_or(0);
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

                    <div class="field">
                        <label for="inactivity_period">"Inactivity Period (minutes)"</label>
                        <input
                            type="number"
                            id="inactivity_period"
                            name="inactivity_period"
                            class="input"
                            min="1"
                            value=move || inactivity_period.get().to_string()
                            on:input=move |ev| {
                                let target = ev.target().unwrap();
                                let input: HtmlInputElement = target.dyn_into().unwrap();
                                if let Ok(value) = input.value().parse() {
                                    set_inactivity_period.set(value);
                                }
                            }
                        />
                    </div>

                    <div class="form-footer">
                        <button
                            type="submit"
                            class="btn btn-primary"
                            disabled=move || loading.get()
                        >
                            {move || if loading.get() { "Saving..." } else { "Save changes" }}
                        </button>
                    </div>
                </form>
            </div>

            <div class="card card-actions">
                <header class="card-header">
                    <h3 class="card-title">"Remote controls"</h3>
                    <p class="card-subtitle">"Send wake and shutdown signals instantly."</p>
                </header>
                <div class="actions-row">
                    <button
                        type="button"
                        class="btn btn-success"
                        on:click=trigger_wake
                        disabled=move || wake_loading.get()
                    >
                        {move || if wake_loading.get() { "Waking..." } else { "Wake machine" }}
                    </button>
                    <Show
                        when=move || {
                            shutdown_control_is_visible(
                                shutdown_setup.get().map(|setup| setup.status),
                            )
                        }
                        fallback=|| view! { <></> }
                    >
                        <button
                            type="button"
                            class="btn btn-danger"
                            on:click=trigger_turn_off
                            disabled=move || turn_off_loading.get() || !can_turn_off_machine.get()
                        >
                            {move || {
                                if turn_off_loading.get() {
                                    "Turning off..."
                                } else {
                                    "Turn off machine"
                                }
                            }}
                        </button>
                    </Show>
                </div>
                {move || {
                    if let Some((success, message)) = wake_feedback.get() {
                        let class = if success {
                            "feedback feedback--success"
                        } else {
                            "feedback feedback--danger"
                        }
                            .to_string();
                        view! { <p class=class>{message}</p> }
                    } else {
                        let class = "feedback feedback--hidden".to_string();
                        let empty = String::new();
                        view! { <p class=class>{empty}</p> }
                    }
                }}
                {move || {
                    if let Some((success, message)) = turn_off_feedback.get() {
                        let class = if success {
                            "feedback feedback--success"
                        } else {
                            "feedback feedback--danger"
                        }
                            .to_string();
                        view! { <p class=class>{message}</p> }
                    } else {
                        let class = "feedback feedback--hidden".to_string();
                        let empty = String::new();
                        view! { <p class=class>{empty}</p> }
                    }
                }}
                <Show
                    when=move || {
                        shutdown_control_is_visible(
                            shutdown_setup.get().map(|setup| setup.status),
                        ) && !can_turn_off_machine.get()
                    }
                    fallback=|| view! { <></> }
                >
                    <p class="field-help">
                        "Configure a remote shutdown port on the machine to activate this action."
                    </p>
                </Show>
            </div>

            <div class="card">
                <header class="card-header">
                    <h3 class="card-title">"Access history"</h3>
                    <p class="card-subtitle">"Connections per service over time."</p>
                </header>
                <div class="actions-row">
                    <button
                        type="button"
                        class=move || {
                            if history_bucket.get() == AccessHistoryBucket::Day { "btn btn-primary btn-sm" } else { "btn btn-soft btn-sm" }
                        }
                        on:click=move |_| set_history_bucket.set(AccessHistoryBucket::Day)
                    >
                        "By day"
                    </button>
                    <button
                        type="button"
                        class=move || {
                            if history_bucket.get() == AccessHistoryBucket::Week { "btn btn-primary btn-sm" } else { "btn btn-soft btn-sm" }
                        }
                        on:click=move |_| set_history_bucket.set(AccessHistoryBucket::Week)
                    >
                        "By week"
                    </button>
                    <button
                        type="button"
                        class=move || {
                            if history_bucket.get() == AccessHistoryBucket::Hour { "btn btn-primary btn-sm" } else { "btn btn-soft btn-sm" }
                        }
                        on:click=move |_| set_history_bucket.set(AccessHistoryBucket::Hour)
                    >
                        "By hour"
                    </button>
                    <button
                        type="button"
                        class="btn btn-soft btn-sm"
                        disabled=move || history_loading.get()
                        on:click=move |_| set_history_refresh.update(|n| *n += 1)
                    >
                        {move || if history_loading.get() { "Refreshing..." } else { "Refresh" }}
                    </button>
                </div>
                <canvas id="usage-chart"></canvas>
            </div>

            <Show when=raw_machine_data_is_visible fallback=|| view! { <></> }>
                <div class="card">
                    <header class="card-header">
                        <h3 class="card-title">"Raw machine data"</h3>
                        <p class="card-subtitle">"Debug snapshot of the API payload."</p>
                    </header>
                    <pre class="code-block">
                        {move || {
                            serde_json::to_string_pretty(&machine_details.get())
                                .unwrap_or_default()
                        }}
                    </pre>
                </div>
            </Show>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ServiceAccessHistory;

    fn millis(year: i32, month: u32, day: u32) -> i64 {
        Utc.with_ymd_and_hms(year, month, day, 0, 0, 0)
            .single()
            .expect("valid test date")
            .timestamp_millis()
    }

    #[test]
    fn bucket_history_groups_by_iso_week() {
        let history = AccessHistory {
            services: vec![ServiceAccessHistory {
                name: Some("ssh".into()),
                local_port: 2222,
                target_port: 22,
                timestamps: vec![millis(2024, 1, 1), millis(2024, 1, 7), millis(2024, 1, 8)],
            }],
        };

        let (labels, datasets) = bucket_history(&history, AccessHistoryBucket::Week);

        assert_eq!(labels, vec!["2024-W01", "2024-W02"]);
        assert_eq!(datasets[0]["label"], serde_json::json!("ssh"));
        assert_eq!(datasets[0]["data"], serde_json::json!([2, 1]));
    }

    #[test]
    fn shutdown_setup_message_explains_key_mismatch() {
        assert_eq!(
            shutdown_setup_message(ShutdownSetupStatus::KeyMismatch),
            "The client responded, but its key does not match. Run the setup command again."
        );
    }

    #[test]
    fn shutdown_control_is_visible_only_for_configured_clients() {
        assert!(shutdown_control_is_visible(Some(
            ShutdownSetupStatus::Legacy
        )));
        assert!(shutdown_control_is_visible(Some(
            ShutdownSetupStatus::Verified
        )));

        for status in [
            None,
            Some(ShutdownSetupStatus::Disabled),
            Some(ShutdownSetupStatus::Pending),
            Some(ShutdownSetupStatus::Unreachable),
            Some(ShutdownSetupStatus::KeyMismatch),
        ] {
            assert!(!shutdown_control_is_visible(status));
        }
    }

    #[test]
    fn copy_feedback_uses_clear_button_labels() {
        assert_eq!(CopyFeedback::Idle.label(), "Copy command");
        assert_eq!(CopyFeedback::Copying.label(), "Copying...");
        assert_eq!(CopyFeedback::Copied.label(), "Copied!");
        assert_eq!(CopyFeedback::Failed.label(), "Copy failed");
    }

    #[test]
    fn copy_feedback_uses_visual_status_classes() {
        assert_eq!(CopyFeedback::Idle.button_class(), "btn btn-soft btn-sm");
        assert_eq!(CopyFeedback::Copying.button_class(), "btn btn-soft btn-sm");
        assert_eq!(
            CopyFeedback::Copied.button_class(),
            "btn btn-success btn-sm"
        );
        assert_eq!(CopyFeedback::Failed.button_class(), "btn btn-danger btn-sm");
    }

    #[test]
    fn raw_machine_data_visibility_matches_the_build_profile() {
        assert_eq!(raw_machine_data_is_visible(), cfg!(debug_assertions));
    }

    #[test]
    fn shutdown_poll_backoff_resets_after_success_and_is_capped() {
        assert_eq!(next_shutdown_poll_delay(3_000, false), 6_000);
        assert_eq!(next_shutdown_poll_delay(24_000, false), 30_000);
        assert_eq!(next_shutdown_poll_delay(30_000, false), 30_000);
        assert_eq!(next_shutdown_poll_delay(30_000, true), 3_000);
    }

    #[test]
    fn rotation_failure_message_includes_the_backend_error() {
        assert_eq!(
            rotation_failure_message("disk full"),
            "Failed to rotate shutdown key: disk full"
        );
    }
}
