use super::{
    browser,
    state::DashboardState,
    widgets::{ErrorMessage, Hardware, Icon},
};
use crate::{
    api,
    models::{Machine, ShutdownSetup, ShutdownSetupStatus},
};
use leptos::prelude::*;

fn pending(status: ShutdownSetupStatus) -> bool {
    matches!(
        status,
        ShutdownSetupStatus::Pending
            | ShutdownSetupStatus::Unreachable
            | ShutdownSetupStatus::KeyMismatch
    )
}

#[component]
pub fn SetupPanel(state: DashboardState, machine: Machine, on_back: Callback<()>) -> impl IntoView {
    let machine = StoredValue::new(machine);
    let setup = RwSignal::<Option<ShutdownSetup>>::new(None);
    let error = RwSignal::<Option<String>>::new(None);
    let checking = RwSignal::new(false);
    let loading = RwSignal::new(true);
    let windows = RwSignal::new(false);
    let confirm_rotation = RwSignal::new(false);
    let verified = move || {
        setup
            .get()
            .is_some_and(|setup| setup.status == ShutdownSetupStatus::Verified)
    };
    let needs_setup = move || setup.get().is_some_and(|setup| pending(setup.status));
    let load = Callback::new(move |()| {
        loading.set(true);
        error.set(None);
        leptos::task::spawn_local_scoped_with_cancellation(async move {
            match api::get_shutdown_setup(&machine.get_value().mac).await {
                Ok(value) => setup.set(Some(value)),
                Err(message) => error.set(Some(message)),
            }
            loading.set(false);
        });
    });
    let verify = Callback::new(move |()| {
        if checking.get_untracked()
            || !setup
                .get_untracked()
                .is_some_and(|setup| pending(setup.status))
        {
            return;
        }
        checking.set(true);
        leptos::task::spawn_local_scoped_with_cancellation(async move {
            match api::verify_shutdown_setup(&machine.get_value().mac).await {
                Ok(value) => {
                    if value.status == ShutdownSetupStatus::Verified {
                        state.record(&machine.get_value(), "completed client setup.");
                    }
                    setup.set(Some(value));
                    error.set(None);
                }
                Err(message) => error.set(Some(message)),
            }
            checking.set(false);
        });
    });
    load.run(());
    leptos::task::spawn_local_scoped_with_cancellation(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(5000).await;
            verify.run(());
        }
    });
    let rotate = move |_| {
        if checking.get_untracked() {
            return;
        }
        checking.set(true);
        leptos::task::spawn_local_scoped_with_cancellation(async move {
            match api::rotate_shutdown_key(&machine.get_value().mac).await {
                Ok(value) => {
                    setup.set(Some(value));
                    confirm_rotation.set(false);
                    error.set(None);
                }
                Err(message) => error.set(Some(message)),
            }
            checking.set(false);
        });
    };
    let install = Signal::derive(move || {
        if windows.get() {
            "irm https://wakezilla.dev/install.ps1 | iex"
        } else {
            "curl -fsSL https://wakezilla.dev/install.sh | sh"
        }
        .to_owned()
    });
    let configure = Signal::derive(move || {
        setup
            .get()
            .and_then(|setup| {
                if windows.get() {
                    setup.windows_command
                } else {
                    setup.unix_command
                }
            })
            .unwrap_or_default()
    });
    view! {
        <ol class="setup-progress" aria-label="Setup steps"><li class="complete"><span><Icon name="check"/></span>"Machine added"</li><li class=move || if verified() { "complete" } else { "current" }><span>{move || if verified() { "✓" } else { "2" }}</span>"Set up client"</li><li class=move || if verified() { "complete" } else { "" }><span>{move || if verified() { "✓" } else { "3" }}</span>"Connection"</li></ol>
        <div class="setup-machine"><Hardware kind=machine.get_value().machine_type/><div><strong>{machine.get_value().name}</strong><span>{machine.get_value().ip}</span></div><span class=move || if verified() { "setup-badge verified" } else { "setup-badge" }>{move || if verified() { "Configured" } else { "Pending" }}</span></div>
        <ErrorMessage message=error.into()/>
        <Show when=move || error.get().is_some() && setup.get().is_none()><button class="button" on:click=move |_| load.run(())>"Try again"</button></Show>
        <Show when=move || loading.get()><p class="loading-state" role="status">"Loading setup..."</p></Show>
        <Show when=needs_setup>
            <div class="setup-platforms" role="group" aria-label="Machine operating system"><button class=move || if windows.get() { "" } else { "selected" } aria-pressed=move || (!windows.get()).to_string() on:click=move |_| windows.set(false)><Icon name="terminal"/>"Linux / macOS"</button><button class=move || if windows.get() { "selected" } else { "" } aria-pressed=move || windows.get().to_string() on:click=move |_| windows.set(true)><Icon name="grid"/>"Windows"</button></div>
            <p class="setup-instructions">"On "<strong>{machine.get_value().name}</strong>{move || if windows.get() { ", open PowerShell as an administrator and run the commands below in order." } else { ", open a terminal and run the commands below in order." }}</p>
            <div class="setup-commands"><CommandStep title="1. Install Wakezilla" command=install/><CommandStep title="2. Set up the client" command=configure/></div>
            <div class="setup-connection" class:checking=move || checking.get() role="status" aria-live="polite"><span class="event-symbol"><Icon name="network"/></span><div><strong>{move || if checking.get() { "Checking connection..." } else { "Waiting for setup" }}</strong><p>{move || match setup.get().map(|setup| setup.status) { Some(ShutdownSetupStatus::KeyMismatch) => "The client key does not match. Run the setup command again.", Some(ShutdownSetupStatus::Unreachable) => "The client has not responded yet. Verification will continue automatically.", _ => "After you run the commands, the connection will be checked automatically." }}</p></div></div>
            <div class="modal-actions setup-actions"><button class="button" on:click=move |_| state.dialog.set(None)>"Set up later"</button><button class="button button-primary" disabled=move || checking.get() on:click=move |_| verify.run(())><Icon name="check"/>{move || if checking.get() { "Checking..." } else { "Check connection" }}</button></div>
        </Show>
        <Show when=verified><div class="setup-success"><span class="setup-success-icon"><Icon name="check"/></span><h3>"Client configured"</h3><p>"The client is configured to authenticate dashboard commands."</p></div></Show>
        <Show when=move || setup.get().is_some_and(|setup| setup.status == ShutdownSetupStatus::Legacy)><p class="setup-instructions">"This client uses a legacy setup. Generate a key to enable command authentication."</p></Show>
        <Show when=move || setup.get().is_some_and(|setup| setup.status == ShutdownSetupStatus::Disabled)><p class="setup-instructions">"Enable dashboard shutdown and set the client port in the machine details."</p></Show>
        <Show when=move || setup.get().is_some_and(|setup| matches!(setup.status, ShutdownSetupStatus::Verified | ShutdownSetupStatus::Legacy))>
            <Show when=move || !confirm_rotation.get() fallback=move || view! { <div class="rotation-confirm"><p>"Generate a new key? The current client cannot authenticate until you run the new command."</p><div class="modal-actions"><button class="button" disabled=move || checking.get() on:click=move |_| confirm_rotation.set(false)>"Cancel"</button><button class="button button-primary" disabled=move || checking.get() on:click=rotate>"Generate new key"</button></div></div> }><button class="button" on:click=move |_| confirm_rotation.set(true)>"Set up client again"</button></Show>
        </Show>
        <Show when=move || !needs_setup()><div class="modal-actions setup-actions"><button class="button" on:click=move |_| on_back.run(())>"View machine"</button><button class="button button-primary" on:click=move |_| state.dialog.set(None)>"Done"</button></div></Show>
    }
}

#[component]
fn CommandStep(title: &'static str, command: Signal<String>) -> impl IntoView {
    let copied = RwSignal::new(false);
    let copying = RwSignal::new(false);
    let error = RwSignal::new(None);
    Effect::new(move |_| {
        command.get();
        copied.set(false);
        error.set(None);
    });
    view! { <section class="setup-command"><div class="setup-command-heading"><h3>{title}</h3><button class="button copy-command" aria-label=format!("Copy command: {title}") disabled=move || copying.get() on:click=move |_| {
        let value = command.get_untracked(); copying.set(true);
        leptos::task::spawn_local_scoped_with_cancellation(async move { match browser::copy(&value).await { Ok(()) => { copied.set(true); error.set(None); }, Err(message) => error.set(Some(message)) }; copying.set(false); });
    }><Icon name="copy"/>{move || if copied.get() { "Copied" } else { "Copy" }}</button></div><pre tabindex="0" aria-label=title><code>{command}</code></pre><Show when=move || copied.get()><p class="setup-copy-feedback" role="status">"Command copied."</p></Show><ErrorMessage message=error.into()/></section> }
}
