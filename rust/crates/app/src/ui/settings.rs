//! The settings window: a tab each for voice and video, sounds, appearance and everything else.

use egui::{self, RichText, Ui};

use crate::app::{Action, UiState};
use crate::settings::Settings;
use crate::sounds::Sound;
use crate::theme;
use crate::ui::{menu, widgets};

const WIDTH: f32 = 560.0;
/// How tall the page is, so the window stays put when changing tabs (less in a small window, and it scrolls).
const PAGE_HEIGHT: f32 = 340.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Tab {
    #[default]
    VoiceAndVideo,
    SoundsAndVolume,
    Appearance,
    Misc,
}

const TABS: [(Tab, &str); 4] = [(Tab::VoiceAndVideo, "Voice and Video"), (Tab::SoundsAndVolume, "Sounds and Volume"), (Tab::Appearance, "Appearance"), (Tab::Misc, "Misc")];

/// An open settings window: the edited copy and the device lists, enumerated once when it opened.
pub struct Draft {
    pub tab: Tab,
    pub settings: Settings,
    /// What's saved, for telling whether there's anything to apply.
    pub saved: Settings,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub cameras: Vec<String>,
    /// Running while the user checks their mic; stops when they leave its tab or the window closes.
    pub mic_test: Option<swiftcord_voice::MicTest>,
    /// Running while the user hears a simulated incoming voice; stops when they leave its tab or the window closes.
    pub incoming_test: Option<swiftcord_voice::IncomingTest>,
    /// The incoming test adds network jitter and loss.
    pub rough_network: bool,
    /// "Share my activity" (an account setting), and what it was when last saved.
    pub share_activity: bool,
    pub saved_share_activity: bool,
    /// Opening at sign-in (Windows' setting, not ours), and what it was when last saved.
    pub launch_at_startup: bool,
    pub saved_launch_at_startup: bool,
}

impl Draft {
    fn changed(&self) -> bool {
        self.settings != self.saved || self.share_activity != self.saved_share_activity || self.launch_at_startup != self.saved_launch_at_startup
    }

    /// Saves what's changed; the window stays as it is.
    fn apply(&mut self, actions: &mut Vec<Action>) {
        if self.settings != self.saved {
            actions.push(Action::SaveSettings(self.settings.clone()));
            self.saved = self.settings.clone();
        }
        if self.share_activity != self.saved_share_activity {
            actions.push(Action::SetShareActivity(self.share_activity));
            self.saved_share_activity = self.share_activity;
        }
        if self.launch_at_startup != self.saved_launch_at_startup {
            actions.push(Action::SetLaunchAtStartup(self.launch_at_startup));
            self.saved_launch_at_startup = self.launch_at_startup;
        }
    }
}

/// A level meter from -80 to 0 dBFS with a marker at the voice-activity threshold.
fn meter(ui: &mut Ui, level_db: f32, threshold_db: f32, open: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 10.0), egui::Sense::hover());
    let x = |db: f32| rect.left() + rect.width() * ((db + 80.0) / 80.0).clamp(0.0, 1.0);
    ui.painter().rect_filled(rect, 3, theme::RAIL());
    let fill = egui::Rect::from_min_max(rect.min, egui::pos2(x(level_db), rect.max.y));
    ui.painter().rect_filled(fill, 3, if open { theme::GREEN() } else { theme::YELLOW() });
    let tx = x(threshold_db);
    ui.painter().vline(tx, rect.y_range().expand(3.0), egui::Stroke::new(2.0, theme::HEADER()));
}

/// A volume (0 to 1) as a whole percentage.
fn volume_slider(ui: &mut Ui, volume: &mut f32) -> egui::Response {
    // Rounded first: the slider rounds what it's given and calls that a change, and many levels come back from
    // a float just off a whole number (0.3 × 100), which would count as a change on every frame.
    let mut percent = (*volume * 100.0).round();
    let r = ui.add(egui::Slider::new(&mut percent, 0.0..=100.0).suffix("%").integer());
    if r.changed() {
        *volume = percent / 100.0;
    }
    r
}

/// A group's heading, where a tab has several.
fn heading(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(theme::font_semibold(12.0)).color(theme::MUTED()));
}

fn note(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(theme::FAINT()));
}

/// How wide a dropdown is in a column, leaving a gap before the next.
fn combo_width(ui: &Ui) -> f32 {
    ui.available_width() - 12.0
}

pub fn show(ui: &Ui, uis: &mut UiState, actions: &mut Vec<Action>) {
    let Some(draft) = &mut uis.settings_draft else { return };
    let mut close = false;
    let content = ui.ctx().content_rect();
    let height = (content.height() - 120.0).clamp(120.0, PAGE_HEIGHT);
    menu::window("Settings").pivot(egui::Align2::CENTER_CENTER).default_pos(content.center()).show(ui.ctx(), |ui| {
        ui.set_width(WIDTH);
        close |= menu::title(ui, "Settings", None);
        let tabs = menu::tabs(ui, &mut draft.tab, &TABS);
        if tabs.changed {
            draft.mic_test = None;
            draft.incoming_test = None;
        }
        menu::page(ui, Some(&tabs), |ui| {
            let scroll = egui::ScrollArea::vertical().id_salt(draft.tab).auto_shrink(false).max_height(height).show(ui, |ui| match draft.tab {
                Tab::VoiceAndVideo => voice_and_video(ui, draft, actions),
                Tab::SoundsAndVolume => sounds_and_volume(ui, &mut draft.settings, actions),
                Tab::Appearance => appearance(ui, &mut draft.settings),
                Tab::Misc => misc(ui, draft, actions, &mut close),
            });
            widgets::scrollbar(ui, &scroll);
        });
        menu::buttons(ui, |ui| {
            let changed = draft.changed();
            if ui.add_enabled(changed, menu::button("Apply")).clicked() {
                draft.apply(actions);
            }
            if ui.add(menu::button("Cancel")).clicked() {
                close = true;
            }
            if ui.add(menu::button("OK")).clicked() {
                draft.apply(actions);
                close = true;
            }
        });
    });
    if close {
        uis.settings_draft = None;
    }
}

fn voice_and_video(ui: &mut Ui, draft: &mut Draft, actions: &mut Vec<Action>) {
    ui.columns(2, |c| {
        device_combo(&mut c[0], "Input device", &mut draft.settings.input_device, &draft.inputs, "System default");
        device_combo(&mut c[1], "Output device", &mut draft.settings.output_device, &draft.outputs, "System default");
    });
    ui.add_space(6.0);
    ui.columns(2, |c| {
        if draft.cameras.is_empty() {
            c[0].label("Camera");
            note(&mut c[0], "No camera found.");
        } else {
            device_combo(&mut c[0], "Camera", &mut draft.settings.camera_device, &draft.cameras, "First camera");
        }
    });
    note(ui, "Device changes apply to the next call.");

    ui.add_space(10.0);
    ui.label("Input sensitivity");
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = ui.available_width() - 170.0;
        ui.add(egui::Slider::new(&mut draft.settings.vad_threshold_db, -80.0..=-10.0).suffix(" dB").text("transmit above"));
    });
    note(ui, "Lower values pick up quieter speech, and more background noise.");
    ui.add_space(4.0);
    ui.checkbox(&mut draft.settings.noise_suppression, "Noise suppression (RNNoise)");
    note(ui, "Removes background noise like fans, hum and typing from your microphone, on this PC.");

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let label = if draft.mic_test.is_some() { "Stop Testing" } else { "Mic Test" };
        if ui.button(label).clicked() {
            draft.mic_test = match draft.mic_test.take() {
                Some(_) => None,
                None => {
                    let devices = swiftcord_voice::Devices { input: draft.settings.input_device.clone(), output: draft.settings.output_device.clone() };
                    Some(swiftcord_voice::MicTest::start(devices, draft.settings.vad_threshold_db, draft.settings.noise_suppression))
                }
            };
        }
        note(ui, "Plays your mic back exactly as others would hear it in a call.");
    });
    if let Some(test) = &draft.mic_test {
        test.set_threshold(draft.settings.vad_threshold_db);
        test.set_noise_suppression(draft.settings.noise_suppression);
        meter(ui, test.level_db(), draft.settings.vad_threshold_db, test.transmitting());
        for e in test.errors() {
            actions.push(Action::Notice(e));
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if draft.incoming_test.is_some() {
            if ui.button("Stop Incoming Test").clicked() {
                draft.incoming_test = None;
            }
        } else if ui.button("Incoming Test…").clicked() {
            draft.mic_test = None;
            actions.push(Action::PickIncomingTestWav);
        }
        ui.checkbox(&mut draft.rough_network, "With a rough network");
    });
    note(ui, "Plays a WAV file as if someone in a call were saying it: through the same decoding and playback as a call, without connecting anywhere.");
    if let Some(test) = &draft.incoming_test {
        for e in test.errors() {
            actions.push(Action::Notice(e));
        }
    }
}

fn sounds_and_volume(ui: &mut Ui, s: &mut Settings, actions: &mut Vec<Action>) {
    ui.checkbox(&mut s.message_sounds, "Message sounds");
    ui.checkbox(&mut s.call_sounds, "Call sounds (ringing, people joining and leaving)");
    ui.checkbox(&mut s.join_sounds, "Custom join sounds people have set in Discord");
    note(ui, "Sounds are off while you're deafened or on Do Not Disturb.");

    ui.add_space(10.0);
    // Each heard at its new level when let go (or changed with the keyboard).
    let mut slider = |ui: &mut Ui, label: &str, volume: &mut f32, sample: Sound| {
        ui.label(label);
        ui.spacing_mut().slider_width = ui.available_width() - 70.0;
        let r = volume_slider(ui, volume);
        if r.drag_stopped() || (r.changed() && !r.dragged()) {
            actions.push(Action::PreviewSound(sample, *volume));
        }
    };
    ui.columns(2, |c| {
        slider(&mut c[0], "Messages", &mut s.message_volume, Sound::Message);
        slider(&mut c[1], "Calls", &mut s.call_volume, Sound::Join);
    });
    ui.add_space(6.0);
    ui.columns(2, |c| {
        slider(&mut c[0], "Clicks", &mut s.click_volume, Sound::ButtonPress);
        slider(&mut c[1], "Hovering over lists", &mut s.rollover_volume, Sound::ButtonRollover);
    });

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if ui.button("Open sounds folder").clicked() {
            actions.push(Action::OpenSoundsFolder);
        }
        note(ui, "Sounds can be replaced with your own WAV files.");
    });
}

fn appearance(ui: &mut Ui, s: &mut Settings) {
    ui.columns(2, |c| {
        c[0].label("Colour theme");
        let width = combo_width(&c[0]);
        egui::ComboBox::from_id_salt("Colour theme").width(width).selected_text(s.theme.clone()).show_ui(&mut c[0], |ui| {
            for name in theme::names() {
                ui.selectable_value(&mut s.theme, name.to_owned(), name);
            }
        });
        c[1].label("Transition effect");
        let width = combo_width(&c[1]);
        egui::ComboBox::from_id_salt("Transition effect").width(width).selected_text(s.transition.name()).show_ui(&mut c[1], |ui| {
            for effect in crate::transition::Effect::ALL {
                ui.selectable_value(&mut s.transition, effect, effect.name());
            }
        });
        note(&mut c[1], "Plays when you switch chats and dismiss menus.");
    });
}

fn misc(ui: &mut Ui, draft: &mut Draft, actions: &mut Vec<Action>, close: &mut bool) {
    heading(ui, "NOTIFICATIONS");
    ui.checkbox(&mut draft.settings.notifications, "Desktop notifications for mentions and DMs");

    ui.add_space(12.0);
    heading(ui, "STARTUP");
    ui.checkbox(&mut draft.launch_at_startup, "Launch '08cord when Windows starts");
    ui.add_enabled(draft.launch_at_startup, egui::Checkbox::new(&mut draft.settings.start_minimized, "Start in the tray"));
    note(ui, "Opens when you sign in to Windows. Also listed in Task Manager under Startup apps.");

    ui.add_space(12.0);
    heading(ui, "ACTIVITY PRIVACY");
    ui.checkbox(&mut draft.share_activity, "Share my activity");
    note(ui, "Show the games and music you're playing (from any device signed in to Discord) on your profile and in member lists. This is a Discord account setting.");

    ui.add_space(12.0);
    heading(ui, "ACCOUNT");
    if ui.button(RichText::new("Log out").color(theme::RED())).clicked() {
        actions.push(Action::LogOut);
        *close = true;
    }
}

fn device_combo(ui: &mut Ui, label: &str, value: &mut Option<String>, devices: &[String], default: &str) {
    ui.label(label);
    let text = value.clone().unwrap_or_else(|| default.into());
    egui::ComboBox::from_id_salt(label).width(combo_width(ui)).truncate().selected_text(text).show_ui(ui, |ui| {
        ui.selectable_value(value, None, default);
        for d in devices {
            ui.selectable_value(value, Some(d.clone()), d);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A change plays a sample of the sound, so a slider left alone must never report one.
    #[test]
    fn a_volume_slider_left_alone_never_changes() {
        let ctx = egui::Context::default();
        let levels = (0..=100).map(|n| n as f32 / 100.0).chain([0.300_000_1, 0.149_999_9, 1.0 / 3.0]);
        for start in levels {
            let (mut volume, mut changed) = (start, false);
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                changed = volume_slider(ui, &mut volume).changed();
            });
            out.textures_delta.clear();
            assert!(!changed, "{start} reported a change");
            assert_eq!(volume, start);
        }
    }

    fn draft() -> Draft {
        Draft {
            tab: Tab::default(),
            settings: Settings::default(),
            saved: Settings::default(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            cameras: Vec::new(),
            mic_test: None,
            incoming_test: None,
            rough_network: false,
            share_activity: true,
            saved_share_activity: true,
            launch_at_startup: false,
            saved_launch_at_startup: false,
        }
    }

    #[test]
    fn applying_saves_what_changed_once_and_leaves_nothing_to_apply() {
        let mut d = draft();
        assert!(!d.changed());
        d.settings.theme = "Nord".into();
        d.launch_at_startup = true;
        assert!(d.changed());
        let mut actions = Vec::new();
        d.apply(&mut actions);
        assert!(matches!(actions.as_slice(), [Action::SaveSettings(s), Action::SetLaunchAtStartup(true)] if s.theme == "Nord"));
        assert!(!d.changed());
        actions.clear();
        d.apply(&mut actions);
        assert!(actions.is_empty());
    }
}
