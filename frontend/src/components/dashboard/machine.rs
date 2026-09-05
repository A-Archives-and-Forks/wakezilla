use super::{
    browser,
    history::HistoryPanel,
    setup::SetupPanel,
    state::{DashboardState, MachineStatus, client_ready},
    widgets::{
        ErrorMessage, Hardware, Icon, MACHINE_TYPES, ModalHeader, Status, parse_type, type_value,
    },
};
use crate::{
    api,
    models::{
        Machine, PortForward, ShutdownSetup, ShutdownSetupStatus, UpdateMachinePayload,
        validate_machine_form,
    },
};
use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    Edit,
    Setup,
    History,
    Delete,
    Shutdown,
}

#[derive(Clone)]
struct ServiceDraft {
    id: usize,
    name: ArcRwSignal<String>,
    local: ArcRwSignal<String>,
    target: ArcRwSignal<String>,
}
impl ServiceDraft {
    fn new(id: usize, service: &PortForward) -> Self {
        Self {
            id,
            name: ArcRwSignal::new(service.name.clone().unwrap_or_default()),
            local: ArcRwSignal::new(if service.local_port == 0 {
                String::new()
            } else {
                service.local_port.to_string()
            }),
            target: ArcRwSignal::new(if service.target_port == 0 {
                String::new()
            } else {
                service.target_port.to_string()
            }),
        }
    }
    fn value(&self) -> Result<PortForward, String> {
        let local_port = self
            .local
            .get_untracked()
            .parse::<u16>()
            .ok()
            .filter(|port| *port > 0)
            .ok_or("Enter a local port between 1 and 65535.")?;
        let target_port = self
            .target
            .get_untracked()
            .parse::<u16>()
            .ok()
            .filter(|port| *port > 0)
            .ok_or("Enter a target port between 1 and 65535.")?;
        let name = self.name.get_untracked().trim().to_owned();
        Ok(PortForward {
            name: (!name.is_empty()).then_some(name),
            local_port,
            target_port,
        })
    }
}

#[component]
pub fn MachineDialog(state: DashboardState, initial: Machine, creating: bool) -> impl IntoView {
    let services = RwSignal::new(
        initial
            .port_forwards
            .iter()
            .enumerate()
            .map(|(id, service)| ServiceDraft::new(id, service))
            .collect::<Vec<_>>(),
    );
    let service_sequence = RwSignal::new(initial.port_forwards.len());
    let draft = RwSignal::new(initial.clone());
    let stored = RwSignal::new(initial);
    let creating = RwSignal::new(creating);
    let pane = RwSignal::new(Pane::Edit);
    let saving = RwSignal::new(false);
    let error = RwSignal::new(None);
    let feedback = RwSignal::new(String::new());
    let setup = RwSignal::<Option<ShutdownSetup>>::new(None);
    let setup_error = RwSignal::new(None);
    let setup_revision = RwSignal::new(0u64);
    let load_setup = Callback::new(move |()| {
        if creating.get_untracked() {
            return;
        }
        let mac = stored.get_untracked().mac;
        setup_revision.update(|value| *value += 1);
        let revision = setup_revision.get_untracked();
        leptos::task::spawn_local_scoped_with_cancellation(async move {
            let result = api::get_shutdown_setup(&mac).await;
            if setup_revision.try_get_untracked() != Some(revision) {
                return;
            }
            match result {
                Ok(value) => {
                    setup.set(Some(value));
                    setup_error.set(None);
                }
                Err(message) => setup_error.set(Some(message)),
            }
        });
    });
    load_setup.run(());
    Effect::new(move |_| {
        pane.get();
        browser::focus_dialog();
    });
    let on_back = Callback::new(move |()| {
        pane.set(Pane::Edit);
        load_setup.run(());
    });
    let title = Signal::derive(move || match pane.get() {
        Pane::Setup => "Set up your machine".into(),
        Pane::History => format!("Access history for {}", stored.get().name),
        Pane::Delete => format!("Delete {}?", stored.get().name),
        Pane::Shutdown => format!("Shut down {}?", stored.get().name),
        Pane::Edit => {
            if creating.get() {
                "Add machine".into()
            } else {
                stored.get().name
            }
        }
    });
    let on_submit = move |event: web_sys::SubmitEvent| {
        event.prevent_default();
        if saving.get_untracked() {
            return;
        }
        let mut machine = draft.get_untracked();
        machine.name = machine.name.trim().to_owned();
        machine.ip = machine.ip.trim().to_owned();
        machine.mac = machine.mac.trim().replace('-', ":").to_uppercase();
        machine.description = machine
            .description
            .filter(|description| !description.trim().is_empty());
        let forwards = services.with_untracked(|services| {
            services
                .iter()
                .map(ServiceDraft::value)
                .collect::<Result<Vec<_>, _>>()
        });
        match forwards {
            Ok(values) => machine.port_forwards = values,
            Err(message) => {
                error.set(Some(message));
                return;
            }
        }
        if !validate_machine_form(&machine).is_empty() {
            error.set(Some(
                "Check the name, IPv4 address, MAC address, and client port.".into(),
            ));
            return;
        }
        let is_new = creating.get_untracked();
        let original_mac = stored.get_untracked().mac;
        saving.set(true);
        error.set(None);
        feedback.set(String::new());
        state.spawn(async move {
            let result = if is_new {
                api::create_machine(machine.clone()).await
            } else {
                api::update_machine(
                    &original_mac,
                    &UpdateMachinePayload {
                        machine_type: Some(machine.machine_type),
                        mac: machine.mac.clone(),
                        ip: machine.ip.clone(),
                        name: machine.name.clone(),
                        description: machine.description.clone(),
                        turn_off_port: machine.turn_off_port,
                        can_be_turned_off: machine.can_be_turned_off,
                        inactivity_period: Some(machine.inactivity_period),
                        port_forwards: Some(machine.port_forwards.clone()),
                    },
                )
                .await
            };
            match result {
                Ok(()) => {
                    state.saved(&original_mac, machine.clone(), is_new);
                    state.record(
                        &machine,
                        if is_new {
                            "was added to the dashboard."
                        } else {
                            "had its configuration updated."
                        },
                    );
                    if saving.try_get_untracked().is_some() {
                        stored.set(machine.clone());
                        draft.set(machine.clone());
                        creating.set(false);
                        saving.set(false);
                        feedback.set("Changes saved.".into());
                        setup.set(None);
                        load_setup.run(());
                        if is_new && machine.can_be_turned_off {
                            pane.set(Pane::Setup);
                        }
                    } else {
                        state.notify("Machine saved.", false);
                    }
                }
                Err(message) => {
                    if saving.try_get_untracked().is_some() {
                        error.set(Some(message));
                        saving.set(false);
                    } else {
                        state.notify(message, true);
                    }
                }
            }
        });
    };
    let delete = move |_| {
        if saving.get_untracked() {
            return;
        }
        let machine = stored.get_untracked();
        saving.set(true);
        error.set(None);
        state.spawn(async move {
            match api::delete_machine(&machine.mac).await {
                Ok(()) => {
                    state.status_epoch.update(|epoch| *epoch += 1);
                    state
                        .machines
                        .update(|machines| machines.retain(|existing| existing.mac != machine.mac));
                    state.record(&machine, "was removed from the dashboard.");
                    if saving.try_get_untracked().is_some() {
                        state.dialog.set(None);
                    }
                    state.notify("Machine deleted.", false);
                }
                Err(message) => {
                    if saving.try_get_untracked().is_some() {
                        error.set(Some(message));
                        saving.set(false);
                    } else {
                        state.notify(message, true);
                    }
                }
            }
        });
    };
    let online = move || state.status(&stored.get().mac) == MachineStatus::Online;
    let pending = move || {
        state
            .pending
            .with(|pending| pending.contains_key(&stored.get().mac))
    };
    view! {
        <ModalHeader state title subtitle="" eyebrow="MACHINE"/>
        <Show when=move || pane.get() == Pane::Edit>
            <Show when=move || !creating.get()>
                <div class="detail-top">{move || view! { <Hardware kind=stored.get().machine_type/> }}<div>{move || view! { <Status state mac=stored.get().mac/> }}<p>{move || stored.get().ip}</p></div>
                    <Show when=online fallback=move || view! { <button class="button button-primary" disabled=pending on:click=move |_| state.power(stored.get_untracked(), true)><Icon name="power"/>{move || if pending() { "Please wait..." } else { "Wake machine" }}</button> }>
                        <Show when=move || setup.get().is_some_and(|setup| client_ready(setup.status)) fallback=move || view! { <button class="button" on:click=move |_| pane.set(Pane::Setup)>"Set up client"</button> }><button class="button" disabled=pending on:click=move |_| pane.set(Pane::Shutdown)><Icon name="power"/>{move || if pending() { "Please wait..." } else { "Shut down" }}</button></Show>
                    </Show>
                </div>
                <div class="client-setup-summary" class:verified=move || setup.get().is_some_and(|setup| setup.status == ShutdownSetupStatus::Verified)><span class="event-symbol"><Icon name="terminal"/></span><div><strong>{move || match setup.get().map(|setup| setup.status) {
                    Some(ShutdownSetupStatus::Verified) => "Client configured", Some(ShutdownSetupStatus::Legacy) => "Legacy client", Some(ShutdownSetupStatus::Disabled) => "Client disabled", None => "Client setup", _ => "Setup pending"
                }}</strong><p>"Command installation and authentication."</p></div><button class="button" on:click=move |_| pane.set(Pane::Setup)>"Set up client"<Icon name="arrow"/></button></div>
                <ErrorMessage message=setup_error.into()/>
                <div class="detail-navigation"><button class="button" on:click=move |_| pane.set(Pane::History)><Icon name="activity"/>"Access history"</button><button class="button button-danger" disabled=pending on:click=move |_| { error.set(None); pane.set(Pane::Delete); }>"Delete machine"</button></div>
            </Show>
            <form id="machine-form" on:submit=on_submit>
                <fieldset disabled=move || saving.get()>
                    <div class="field-grid">
                        <div class="field"><label for="machine-name">"Machine name"</label><input id="machine-name" required maxlength="80" autocomplete="off" prop:value=move || draft.get().name on:input=move |event| draft.update(|machine| machine.name = event_target_value(&event))/></div>
                        <div class="field"><label for="machine-type">"Machine type"</label><select id="machine-type" prop:value=move || type_value(draft.get().machine_type) on:change=move |event| draft.update(|machine| machine.machine_type = parse_type(&event_target_value(&event)))>{MACHINE_TYPES.into_iter().map(|(_,value,label)| view! { <option value=value>{label}</option> }).collect_view()}</select></div>
                        <div class="field"><label for="machine-ip">"IP address"</label><input id="machine-ip" required inputmode="decimal" placeholder="192.168.1.50" prop:value=move || draft.get().ip on:input=move |event| draft.update(|machine| machine.ip = event_target_value(&event))/></div>
                        <div class="field"><label for="machine-mac">"MAC address"</label><input id="machine-mac" required pattern="([0-9a-fA-F]{2}[:-]){5}[0-9a-fA-F]{2}" title="Use six hexadecimal pairs separated by colons." placeholder="AA:BB:CC:DD:EE:FF" prop:value=move || draft.get().mac on:input=move |event| draft.update(|machine| machine.mac = event_target_value(&event))/></div>
                        <div class="field full"><label for="machine-description">"Description "<span class="optional-label">"optional"</span></label><textarea id="machine-description" rows="2" maxlength="240" prop:value=move || draft.get().description.unwrap_or_default() on:input=move |event| draft.update(|machine| machine.description = Some(event_target_value(&event)))></textarea></div>
                    </div>
                    <details class="advanced-settings" open=move || !creating.get()><summary>"Client settings"</summary><div class="field-grid">
                        <label class="checkbox-field full"><input id="machine-shutdown-enabled" type="checkbox" prop:checked=move || draft.get().can_be_turned_off on:change=move |event| draft.update(|machine| { machine.can_be_turned_off = event_target_checked(&event); if machine.can_be_turned_off && machine.turn_off_port.is_none() { machine.turn_off_port = Some(3001); } })/>"Allow shutdown from the dashboard"</label>
                        <div class="field"><label for="machine-client-port">"Client port"</label><input id="machine-client-port" type="number" min="1" max="65535" required=move || draft.get().can_be_turned_off prop:value=move || draft.get().turn_off_port.map(|port| port.to_string()).unwrap_or_default() on:input=move |event| draft.update(|machine| machine.turn_off_port = event_target_value(&event).parse().ok())/><small>"Default: 3001. It must match the client port."</small></div>
                        <div class="field"><label for="machine-idle">"Inactivity (minutes)"</label><input id="machine-idle" type="number" min="0" max="4294967295" step="1" required prop:value=move || draft.get().inactivity_period.to_string() on:input=move |event| { if let Ok(minutes) = event_target_value(&event).parse::<u32>() { draft.update(|machine| machine.inactivity_period = minutes); } }/><small>"Use 0 to disable automatic shutdown only."</small></div>
                    </div></details>
                    <section class="service-editor"><div class="service-editor-heading"><h3>"Services and port forwarding"</h3><button type="button" class="button" on:click=move |_| { let id = service_sequence.get_untracked(); service_sequence.update(|value| *value += 1); services.update(|services| services.push(ServiceDraft::new(id, &PortForward { name: None, local_port:0, target_port:0 }))); }><Icon name="plus"/>"Add service"</button></div><div class="service-rows"><For each=move || services.get() key=|service| service.id children=move |service| view! { <ServiceRow service services/> }/></div><Show when=move || services.get().is_empty()><p class="services-empty">"No services configured. Add port forwarding when you need it."</p></Show></section>
                </fieldset>
                <ErrorMessage message=error.into()/><p class="edit-feedback" role="status">{move || feedback.get()}</p>
                <div class="modal-actions edit-actions"><button type="button" class="button" disabled=move || saving.get() on:click=move |_| state.dialog.set(None)>"Cancel"</button><button type="submit" class="button button-primary" disabled=move || saving.get()><Icon name="check"/>{move || if saving.get() { "Saving..." } else if creating.get() { "Add machine" } else { "Save changes" }}</button></div>
            </form>
        </Show>
        {move || match pane.get() {
            Pane::Setup => view! { <SetupPanel state machine=stored.get_untracked() on_back/> }.into_any(),
            Pane::History => view! { <HistoryPanel mac=stored.get_untracked().mac on_back/> }.into_any(),
            Pane::Delete => view! { <p class="about-text">"The machine and its port forwarding rules will be removed from the dashboard. This action does not shut down the machine."</p><ErrorMessage message=error.into()/><div class="modal-actions"><button class="button" disabled=move || saving.get() on:click=move |_| pane.set(Pane::Edit)>"Cancel"</button><button class="button button-danger" disabled=move || saving.get() on:click=delete>"Delete machine"</button></div> }.into_any(),
            Pane::Shutdown => view! { <p class="about-text">"This machine's services will be unavailable after shutdown."</p><div class="modal-actions"><button class="button" on:click=move |_| pane.set(Pane::Edit)>"Back"</button><button class="button button-primary" disabled=pending on:click=move |_| { state.power(stored.get_untracked(), false); pane.set(Pane::Edit); }>"Shut down machine"</button></div> }.into_any(),
            Pane::Edit => ().into_any(),
        }}
    }
}

#[component]
fn ServiceRow(service: ServiceDraft, services: RwSignal<Vec<ServiceDraft>>) -> impl IntoView {
    let id = service.id;
    let name = service.name.clone();
    let local = service.local.clone();
    let target = service.target.clone();
    view! { <div class="service-edit-row"><div class="field service-name-field"><label for=format!("service-name-{id}")>"Service name"</label><input id=format!("service-name-{id}") placeholder="Example: Plex" prop:value=move || service.name.get() on:input=move |event| name.set(event_target_value(&event))/></div><div class="field"><label for=format!("service-local-{id}")>"Local port"</label><input id=format!("service-local-{id}") type="number" min="1" max="65535" required prop:value=move || service.local.get() on:input=move |event| local.set(event_target_value(&event))/></div><span class="port-direction"><Icon name="arrow"/></span><div class="field"><label for=format!("service-target-{id}")>"Target port"</label><input id=format!("service-target-{id}") type="number" min="1" max="65535" required prop:value=move || service.target.get() on:input=move |event| target.set(event_target_value(&event))/></div><button type="button" class="icon-button service-remove" aria-label="Remove service" on:click=move |_| services.update(|services| services.retain(|service| service.id != id))><Icon name="close"/></button></div> }
}
