#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_os = "windows"))]
compile_error!("LimitPeeker supports Windows 11 only.");

mod advice;
mod agy;
mod agy_session;
mod boxes;
mod choice;
mod claude;
mod claude_home;
mod claude_session;
mod claude_usage;
mod cli_update;
mod codex;
mod conpty;
mod grok;
mod icons;
mod job;
mod json;
mod layout;
mod log;
mod logon_task;
mod message;
mod notify;
mod places;
mod popup;
#[cfg(test)]
mod program_icon;
mod providers;
mod remove;
mod resources;
mod restart;
mod retry;
mod scene;
mod settings;
mod settings_link;
mod settings_words;
mod shell;
mod startup;
mod style;
mod tray_icon;
mod usage;
mod windows_app;
mod xaml;
mod xaml_layout;

fn main() -> windows::core::Result<()> {
    windows_app::run()
}
