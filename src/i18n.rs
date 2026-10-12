// SPDX-License-Identifier: GPL-3.0-only

//! The applet's words, in English or Portuguese, as the session's language.

use std::sync::LazyLock;

pub struct Strings {
    pub nothing_playing: &'static str,
    pub nothing_playing_about: &'static str,
    pub timer: &'static str,
    pub timer_done: &'static str,
    pub timer_done_body: &'static str,
    pub start_timer: &'static str,
    pub paused: &'static str,
    pub charging: &'static str,
    pub charged: &'static str,
    pub until_full: &'static str,
    pub recording: &'static str,
    pub customize: &'static str,
    pub on_the_panel: &'static str,
    pub show_cover: &'static str,
    pub show_title: &'static str,
    pub title_width: &'static str,
    pub small: &'static str,
    pub medium: &'static str,
    pub large: &'static str,
    pub show_bars: &'static str,
    pub activities: &'static str,
    pub media: &'static str,
    pub recording_activity: &'static str,
    pub charging_activity: &'static str,
}

static EN: Strings = Strings {
    nothing_playing: "Nothing playing",
    nothing_playing_about: "Play something in any app and it shows up here",
    timer: "Timer",
    timer_done: "Time's up",
    timer_done_body: "Your timer is done",
    start_timer: "Start a timer",
    paused: "Paused",
    charging: "Charging",
    charged: "Charged",
    until_full: "until full",
    recording: "Recording",
    customize: "Customize",
    on_the_panel: "On the panel",
    show_cover: "Cover",
    show_title: "Song title",
    title_width: "Title width",
    small: "Small",
    medium: "Medium",
    large: "Large",
    show_bars: "Animated bars",
    activities: "Activities",
    media: "Media",
    recording_activity: "Apps recording",
    charging_activity: "Charging",
};

static PT: Strings = Strings {
    nothing_playing: "Nada tocando",
    nothing_playing_about: "Toque algo em qualquer app e ele aparece aqui",
    timer: "Timer",
    timer_done: "Acabou o tempo",
    timer_done_body: "Seu timer terminou",
    start_timer: "Iniciar um timer",
    paused: "Pausado",
    charging: "Carregando",
    charged: "Carregada",
    until_full: "para carregar",
    recording: "Gravando",
    customize: "Personalizar",
    on_the_panel: "No painel",
    show_cover: "Capa",
    show_title: "Título da música",
    title_width: "Largura do título",
    small: "Pequena",
    medium: "Média",
    large: "Grande",
    show_bars: "Barras animadas",
    activities: "Atividades",
    media: "Mídia",
    recording_activity: "Apps gravando",
    charging_activity: "Carregamento",
};

pub fn strings() -> &'static Strings {
    static CHOSEN: LazyLock<&'static Strings> = LazyLock::new(|| {
        let language = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()))
            .unwrap_or_default();
        if language.starts_with("pt") { &PT } else { &EN }
    });
    *CHOSEN
}
