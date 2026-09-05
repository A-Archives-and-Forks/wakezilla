use crate::{
    api,
    models::{Machine, ShutdownSetupStatus},
};
use leptos::prelude::*;
use std::{collections::HashMap, future::Future};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineStatus {
    Checking,
    Online,
    Unreachable,
    Error,
}
impl MachineStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Checking => "Checking...",
            Self::Online => "Online",
            Self::Unreachable => "Unreachable",
            Self::Error => "Status unavailable",
        }
    }
}

#[derive(Clone)]
pub enum DialogView {
    Machine {
        machine: Box<Machine>,
        creating: bool,
    },
    Scan,
}
#[derive(Clone)]
pub struct Activity {
    pub machine: String,
    pub message: String,
    pub time: String,
}

#[derive(Clone, Copy)]
pub struct DashboardState {
    pub machines: RwSignal<Vec<Machine>>,
    pub statuses: RwSignal<HashMap<String, MachineStatus>>,
    pub pending: RwSignal<HashMap<String, bool>>,
    pub dialog: RwSignal<Option<DialogView>>,
    pub events: RwSignal<Vec<Activity>>,
    pub loading: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
    pub refresh: RwSignal<u64>,
    pub status_epoch: RwSignal<u64>,
    pub toast: RwSignal<Option<(String, bool)>>,
    toast_id: RwSignal<u64>,
    owner: StoredValue<Option<Owner>>,
}
impl DashboardState {
    pub fn new() -> Self {
        Self {
            machines: RwSignal::new(vec![]),
            statuses: RwSignal::new(HashMap::new()),
            pending: RwSignal::new(HashMap::new()),
            dialog: RwSignal::new(None),
            events: RwSignal::new(vec![]),
            loading: RwSignal::new(true),
            error: RwSignal::new(None),
            refresh: RwSignal::new(0),
            status_epoch: RwSignal::new(0),
            toast: RwSignal::new(None),
            toast_id: RwSignal::new(0),
            owner: StoredValue::new(Owner::current()),
        }
    }
    pub fn spawn(self, future: impl Future<Output = ()> + 'static) {
        self.owner.with_value(move |owner| {
            if let Some(owner) = owner {
                owner.with(move || leptos::task::spawn_local_scoped_with_cancellation(future));
            } else {
                leptos::task::spawn_local(future);
            }
        });
    }
    pub fn open(self, machine: Machine, creating: bool) {
        self.dialog.set(Some(DialogView::Machine {
            machine: Box::new(machine),
            creating,
        }));
    }
    pub fn status(self, mac: &str) -> MachineStatus {
        self.statuses
            .with(|items| items.get(mac).copied().unwrap_or(MachineStatus::Checking))
    }
    pub fn record(self, machine: &Machine, message: &str) {
        self.events.update(|events| {
            events.insert(
                0,
                Activity {
                    machine: machine.name.clone(),
                    message: message.into(),
                    time: chrono::Local::now().format("%H:%M").to_string(),
                },
            );
            events.truncate(100);
        });
    }
    pub fn notify(self, message: impl Into<String>, error: bool) {
        self.toast_id.update(|id| *id += 1);
        let id = self.toast_id.get_untracked();
        self.toast.set(Some((message.into(), error)));
        self.spawn(async move {
            gloo_timers::future::TimeoutFuture::new(7000).await;
            if self.toast_id.get_untracked() == id {
                self.toast.set(None);
            }
        });
    }
    pub fn saved(self, original_mac: &str, machine: Machine, creating: bool) {
        self.status_epoch.update(|epoch| *epoch += 1);
        self.machines.update(|machines| {
            if creating {
                machines.insert(0, machine);
            } else if let Some(existing) = machines
                .iter_mut()
                .find(|existing| existing.mac == original_mac)
            {
                *existing = machine;
            }
        });
    }
    pub fn power(self, machine: Machine, wake: bool) {
        if self
            .pending
            .with_untracked(|pending| pending.contains_key(&machine.mac))
        {
            return;
        }
        self.pending.update(|pending| {
            pending.insert(machine.mac.clone(), wake);
        });
        self.status_epoch.update(|epoch| *epoch += 1);
        self.spawn(async move {
            let result = if wake {
                api::wake_machine(&machine.mac).await
            } else {
                api::turn_off_machine(&machine.mac).await
            };
            if let Err(error) = result {
                self.pending.update(|pending| {
                    pending.remove(&machine.mac);
                });
                self.notify(error, true);
                return;
            }
            self.record(
                &machine,
                if wake {
                    "received a wake command."
                } else {
                    "received a shutdown command."
                },
            );
            self.notify("Command sent. Waiting for the machine to respond...", false);
            for _ in 0..20 {
                gloo_timers::future::TimeoutFuture::new(3000).await;
                if let Ok(online) = api::get_machine_status(&machine.mac).await
                    && online == wake
                {
                    self.status_epoch.update(|epoch| *epoch += 1);
                    self.statuses.update(|statuses| {
                        statuses.insert(
                            machine.mac.clone(),
                            if online {
                                MachineStatus::Online
                            } else {
                                MachineStatus::Unreachable
                            },
                        );
                    });
                    self.pending.update(|pending| {
                        pending.remove(&machine.mac);
                    });
                    self.notify(
                        if online {
                            format!("{} is online.", machine.name)
                        } else {
                            format!("{} stopped responding to the dashboard.", machine.name)
                        },
                        false,
                    );
                    return;
                }
            }
            self.pending.update(|pending| {
                pending.remove(&machine.mac);
            });
            self.notify(
                "The command was sent, but the status change has not been confirmed yet.",
                true,
            );
        });
    }
}

pub fn client_ready(status: ShutdownSetupStatus) -> bool {
    matches!(
        status,
        ShutdownSetupStatus::Verified | ShutdownSetupStatus::Legacy
    )
}
