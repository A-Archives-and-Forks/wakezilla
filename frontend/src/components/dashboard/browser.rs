use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(inline_js = r#"
export async function copy_command(value) {
    if (navigator.clipboard && window.isSecureContext) {
        try { await navigator.clipboard.writeText(value); return; } catch {}
    }
    const textarea = document.createElement('textarea');
    textarea.value = value;
    textarea.setAttribute('readonly', '');
    textarea.style.cssText = 'position:fixed;top:0;left:0;width:1px;height:1px;opacity:0';
    const active = document.activeElement;
    (document.querySelector('dialog[open]') || document.body).appendChild(textarea);
    try {
        textarea.select();
        textarea.setSelectionRange(0, value.length);
        if (!document.execCommand('copy')) throw new Error('copy failed');
    } finally { textarea.remove(); if (active?.isConnected) active.focus({preventScroll:true}); }
}
export function apply_theme(light) {
    document.documentElement.dataset.theme = light ? 'light' : 'dark';
    document.querySelector('meta[name="theme-color"]').content = light ? '#edf2e9' : '#1c292b';
    try { localStorage.setItem('wakezilla-theme', light ? 'light' : 'dark'); } catch {}
}
export function initial_theme() { return document.documentElement.dataset.theme === 'light'; }
export function focus_dialog() {
    const dialog = document.querySelector('dialog[open]');
    if (!dialog) return;
    dialog.scrollTop = 0;
    dialog.querySelector('#modal-title')?.focus({preventScroll:true});
}
"#)]
extern "C" {
    fn copy_command(value: &str) -> js_sys::Promise;
    pub fn apply_theme(light: bool);
    pub fn initial_theme() -> bool;
    pub fn focus_dialog();
}
pub async fn copy(value: &str) -> Result<(), String> {
    JsFuture::from(copy_command(value))
        .await
        .map(|_| ())
        .map_err(|_| "Could not copy. Select the command and copy it manually.".to_owned())
}
