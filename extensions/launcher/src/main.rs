use eframe::egui::*;
use std::process::Command;

const FALLBACK_PROMPTS: [&str; 4] = [
    "Ask me something…",
    "What can you help with?",
    "Type a question…",
    "Try asking me anything",
];

struct LauncherApp {
    input_text: String,
    current_prompt: String,
    current_theme_colors: (Color32, Color32),
}

impl LauncherApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let prompt = Self::fetch_prompt();
        let theme = std::env::var("LAUNCHER_THEME")
            .ok()
            .unwrap_or_else(|| "dark".to_string());
        let colors = Self::theme_colors(&theme);

        Self {
            input_text: String::new(),
            current_prompt: prompt,
            current_theme_colors: colors,
        }
    }

    fn theme_colors(key: &str) -> (Color32, Color32) {
        let theme = match key {
            "paper" => "paper",
            "matcha" => "matcha",
            _ => "slate", // "slate" or "dark" both map to dark
        };
        match theme {
            "paper" => (Color32::from_rgb(244, 246, 248), Color32::from_rgb(22, 24, 29)),
            "matcha" => (Color32::from_rgb(29, 30, 24), Color32::from_rgb(159, 187, 159)),
            _ => (Color32::from_rgb(15, 23, 42), Color32::from_rgb(203, 213, 225)),
        }
    }

    fn fetch_prompt() -> String {
        let backend_url = std::env::var("BACKEND_URL")
            .ok()
            .or_else(|| std::env::var("LLM_TULPA_BACKEND_URL").ok());

        if let Some(base_url) = backend_url {
            let url = format!("{}/api/prompts/input_examples", base_url);
            let client = reqwest::blocking::Client::new();
            if let Ok(resp) = client.post(&url).header("Accept", "application/json").timeout(std::time::Duration::from_millis(2000)).send() {
                if let Ok(json) = resp.text() {
                    if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&json) {
                        if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                return text.to_string();
                            }
                        }
                    }
                }
            }
        }

        let fallback_index = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as usize;
        FALLBACK_PROMPTS[fallback_index % FALLBACK_PROMPTS.len()].to_string()
    }

    fn percent_encode(s: &str) -> String {
        let mut result = String::new();
        for ch in s.chars() {
            match ch {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '.' | '_' | '~' | '/' | ':' | '@' | '!' | '$' | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '=' | '?' => {
                    result.push(ch);
                }
                ' ' => result.push_str("%20"),
                _ => {
                    for byte in ch.to_string().as_bytes() {
                        result.push('%');
                        result.push_str(&format!("{:02X}", byte));
                    }
                }
            }
        }
        result
    }

    fn open_browser(&mut self) {
        let encoded = Self::percent_encode(&self.input_text);
        let frontend_url = std::env::var("FRONTEND_URL")
            .ok()
            .or_else(|| std::env::var("VITE_FRONTEND_URL").ok())
            .unwrap_or_else(|| "http://localhost:5173".to_string());
        let url = format!("{}/?prompt={}", frontend_url, encoded);
        let _ = Command::new("xdg-open").arg(&url).spawn();
        std::process::exit(0);
    }
}

impl eframe::App for LauncherApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let (bg, fg) = self.current_theme_colors;
        let available = ui.max_rect();

        // Paint background
        ui.painter().add(egui::Shape::rect_filled(
            Rect::from_min_size(Pos2::ZERO, available.size()),
            0.0, bg
        ));

        // Theme styling
        ui.style_mut().visuals.panel_fill = bg;
        ui.style_mut().visuals.override_text_color = Some(fg);
        ui.style_mut().visuals.widgets.inactive.bg_fill = bg;
        ui.style_mut().visuals.widgets.inactive.weak_bg_fill = bg;
        ui.style_mut().visuals.widgets.inactive.fg_stroke = Stroke { width: 1.0, color: fg };
        ui.style_mut().visuals.widgets.open.bg_fill = bg;
        ui.style_mut().visuals.widgets.open.weak_bg_fill = bg;
        ui.style_mut().visuals.widgets.open.fg_stroke = Stroke { width: 1.0, color: fg };
        ui.style_mut().visuals.widgets.hovered.bg_fill = fg.linear_multiply(0.1);
        ui.style_mut().visuals.widgets.hovered.weak_bg_fill = bg;
        ui.style_mut().visuals.widgets.hovered.fg_stroke = Stroke { width: 1.0, color: fg };
        ui.style_mut().visuals.widgets.active.bg_fill = fg.linear_multiply(0.1);
        ui.style_mut().visuals.widgets.active.weak_bg_fill = bg;
        ui.style_mut().visuals.widgets.active.fg_stroke = Stroke { width: 1.0, color: fg };
        ui.style_mut().visuals.selection.bg_fill = fg.linear_multiply(0.2);

        // Center input with 4px margin on all sides.
        // Painters don't advance the cursor, so after painter() the cursor is at (0,0).
        // Use spacers to push content to the center.
        let margin = 4.0;
        let content_height = 32.0;
        let content_width = available.width() - margin * 2.0;

        let vertical_space = available.height() - content_height;
        let top_space = vertical_space / 2.0;
        let bottom_space = vertical_space - top_space;

        let horizontal_space = available.width() - content_width;
        let left_space = horizontal_space / 2.0;
        let right_space = horizontal_space - left_space;

        if top_space > 0.0 {
            ui.allocate_space(Vec2::new(0.0, top_space));
        }

        ui.horizontal(|ui| {
            if left_space > 0.0 {
                ui.allocate_space(Vec2::new(left_space, 0.0));
            }

            ui.add(TextEdit::singleline(&mut self.input_text)
                .hint_text(&self.current_prompt)
                .desired_width(content_width)
                .background_color(bg));

            if ui.input(|i| i.key_pressed(Key::Enter)) && !self.input_text.trim().is_empty() {
                self.open_browser();
            }
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                self.input_text.clear();
            }

            if right_space > 0.0 {
                ui.allocate_space(Vec2::new(right_space, 0.0));
            }
        });

        if bottom_space > 0.0 {
            ui.allocate_space(Vec2::new(0.0, bottom_space));
        }
    }
}

fn main() -> eframe::Result {
    dotenv::dotenv().ok();

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_inner_size(vec2(620.0, 56.0))
            .with_resizable(false),
        ..Default::default()
    };

    eframe::run_native("Launcher", options, Box::new(|cc| {
        Ok(Box::new(LauncherApp::new(cc)))
    }))
}
