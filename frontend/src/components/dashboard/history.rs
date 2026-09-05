use super::widgets::{ErrorMessage, Icon};
use crate::{api, models::AccessHistory};
use leptos::prelude::*;
use std::collections::BTreeMap;

fn bucket_history(history: &AccessHistory, period: &str) -> Vec<(i64, Vec<usize>)> {
    let hour = 3_600_000i64;
    let (width, offset) = match period {
        "hour" => (hour, 0),
        "week" => (7 * 24 * hour, 3 * 24 * hour),
        _ => (24 * hour, 0),
    };
    let mut buckets = BTreeMap::new();
    for (index, service) in history.services.iter().enumerate() {
        for &timestamp in &service.timestamps {
            let start = (timestamp + offset).div_euclid(width) * width - offset;
            buckets
                .entry(start)
                .or_insert_with(|| vec![0; history.services.len()])[index] += 1;
        }
    }
    buckets.into_iter().collect()
}
fn format_time(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp_millis(timestamp)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%d/%m/%Y %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "—".into())
}

#[component]
pub fn HistoryPanel(mac: String, on_back: Callback<()>) -> impl IntoView {
    let mac = StoredValue::new(mac);
    let history = RwSignal::<Option<AccessHistory>>::new(None);
    let error = RwSignal::new(None);
    let loading = RwSignal::new(true);
    let period = RwSignal::new("day");
    let stacked = RwSignal::new(true);
    let load = Callback::new(move |()| {
        loading.set(true);
        error.set(None);
        leptos::task::spawn_local_scoped_with_cancellation(async move {
            match api::get_access_history(&mac.get_value()).await {
                Ok(value) => history.set(Some(value)),
                Err(message) => error.set(Some(message)),
            }
            loading.set(false);
        });
    });
    load.run(());
    let buckets = Memo::new(move |_| {
        history
            .get()
            .map(|history| bucket_history(&history, period.get()))
            .unwrap_or_default()
    });
    let maximum = move || {
        buckets
            .get()
            .iter()
            .map(|(_, values)| {
                if stacked.get() {
                    values.iter().sum()
                } else {
                    values.iter().copied().max().unwrap_or(0)
                }
            })
            .max()
            .unwrap_or(1)
            .max(1)
    };
    view! {
        <div class="detail-navigation"><button class="button" on:click=move |_| on_back.run(())>"Back to machine"</button><button class="button" disabled=move || loading.get() on:click=move |_| load.run(())><Icon name="activity"/>"Refresh history"</button></div>
        <ErrorMessage message=error.into()/>
        <Show when=move || loading.get()><p class="loading-state" role="status">"Loading access history..."</p></Show>
        <Show when=move || history.get().is_some()>
            <p class="setup-instructions">"Connections received through port forwarding. Counts match the records available on the server."</p>
            <div class="history-controls"><div class="filters" role="group" aria-label="Group access records">{[("hour","Hour"),("day","Day"),("week","Week")].into_iter().map(|(value,label)| view! { <button class=move || if period.get() == value { "filter active" } else { "filter" } aria-pressed=move || (period.get() == value).to_string() on:click=move |_| period.set(value)>{label}</button> }).collect_view()}</div><label class="checkbox-field"><input type="checkbox" prop:checked=move || stacked.get() on:change=move |event| stacked.set(event_target_checked(&event))/>"Stack services"</label></div>
            <Show when=move || !buckets.get().is_empty() fallback=|| view! { <p class="services-empty">"No access records yet."</p> }>
                <div class="history-chart-scroll" tabindex="0" aria-label="Access chart; scroll to view all intervals"><div class="history-chart" role="img" aria-label="Access records per service, grouped by interval">{move || {
                    let max = maximum() as f64;
                    buckets.get().into_iter().map(|(timestamp, values)| {
                        let label = chrono::DateTime::from_timestamp_millis(timestamp).map(|time| time.format(if period.get() == "hour" { "%d/%m %Hh" } else { "%d/%m" }).to_string()).unwrap_or_default();
                        let total: usize = values.iter().sum();
                        let title = format!("{} UTC: {} access records", label, total);
                        view! { <div class="history-column" title=title><div class=move || if stacked.get() { "history-bars stacked" } else { "history-bars" }>{values.into_iter().enumerate().map(|(index,count)| view! { <span class="history-bar" style:height=format!("{}px", count as f64 / max * 140.0) style:background=format!("var(--history-series-{})", index % 6) title=format!("Service {}: {} access records", index+1, count)></span> }).collect_view()}</div><small>{label}</small></div> }
                    }).collect_view()
                }}</div></div><p class="history-caption">"Intervals use UTC. Hover over a bar to view its value."</p>
            </Show>
            <div class="table-scroll"><table class="history-table"><thead><tr><th>"Service"</th><th>"Access records"</th><th>"Last access"</th></tr></thead><tbody>{move || history.get().map(|history| history.services.into_iter().enumerate().map(|(index,service)| view! { <tr><td><span class="history-legend" style:background=format!("var(--history-series-{})",index % 6)></span>{service.name.unwrap_or_else(|| format!("Port {}",service.local_port))}<small>{format!("{} → {}", service.local_port, service.target_port)}</small></td><td>{service.timestamps.len()}</td><td>{service.timestamps.iter().max().map(|timestamp| format_time(*timestamp)).unwrap_or_else(|| "No access records".into())}</td></tr> }).collect_view())}</tbody></table></div>
            <details class="advanced-settings"><summary>"History data"</summary><pre class="history-raw">{move || history.get().and_then(|history| serde_json::to_string_pretty(&history).ok())}</pre></details>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ServiceAccessHistory;
    use chrono::TimeZone;
    #[test]
    fn weekly_history_preserves_per_service_counts_and_starts_on_monday() {
        let monday = chrono::Utc
            .with_ymd_and_hms(2026, 8, 31, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let history = AccessHistory {
            services: vec![
                ServiceAccessHistory {
                    name: None,
                    local_port: 1234,
                    target_port: 80,
                    timestamps: vec![monday, monday + 86_400_000],
                },
                ServiceAccessHistory {
                    name: None,
                    local_port: 1235,
                    target_port: 81,
                    timestamps: vec![monday + 2 * 86_400_000],
                },
            ],
        };
        assert_eq!(bucket_history(&history, "week"), vec![(monday, vec![2, 1])]);
    }
}
