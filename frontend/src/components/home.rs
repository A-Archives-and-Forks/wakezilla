use leptos::prelude::*;
use std::collections::HashMap;

use leptos::leptos_dom::logging::console_log;

use crate::api::{fetch_machines, is_machine_online};
use crate::components::{AddMachine, Header, RegisteredMachines};
use crate::models::{Machine, PortForward};

#[component]
pub fn HomePage() -> impl IntoView {
    let default_machine = Machine {
        turn_off_port: Some(3000),
        port_forwards: vec![PortForward {
            name: None,
            local_port: 0,
            target_port: 0,
        }],
        ..Default::default()
    };
    let (machine, set_machine) = signal::<Machine>(default_machine);

    let (registred_machines, set_registred_machines) = signal::<Vec<Machine>>(vec![]);
    let (status_machine, set_status_machine) = signal::<HashMap<String, bool>>(HashMap::new());

    // Load initial registred machines
    Effect::new(move || {
        leptos::task::spawn_local(async move {
            if let Ok(machines) = fetch_machines().await {
                //console_log(&format!("Loaded {:?} machines", machines));
                set_registred_machines.set(machines);
            }
        });
    });

    // check the status of registred machines when they change
    Effect::new(move |_| {
        let machines = registred_machines.get();
        if machines.is_empty() {
            // console_log("No registred machines");
            return;
        }

        // Spawn the async task to check all machines
        leptos::task::spawn_local(async move {
            // Create a vector of futures to check each machine concurrently
            let mut futures = Vec::new();

            for m in machines {
                let machine_mac = m.mac.clone();
                let machine_name = m.name.clone();

                console_log(&format!("Checking machine {}", machine_name));
                let future = async move { (machine_mac, is_machine_online(&m.mac).await) };
                futures.push(future);
            }

            // Wait for all futures to complete, regardless of individual failures
            let results = futures::future::join_all(futures).await;

            // Build the status map from all results
            let mut status_map = HashMap::new();
            for (mac, is_online) in results {
                status_map.insert(mac, is_online);
            }
            set_status_machine.set(status_map);
        });
    });

    view! {
        <Header set_machine=set_machine registred_machines=registred_machines />
        <Show when=move || { !registred_machines.get().is_empty() } fallback=|| view! {}>
            <RegisteredMachines
                machines=registred_machines
                status_machine=status_machine
                set_registred_machines=set_registred_machines
            />
        </Show>
        <AddMachine
            machine=machine
            registred_machines=registred_machines
            set_registred_machines=set_registred_machines
        />
    }
}
