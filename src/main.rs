use axum::{
    body::Bytes,
    extract::{State, OriginalUri},
    http::{HeaderMap, Method},
    response::IntoResponse,
    routing::any,
    Router,
};
use eframe::egui;
use reqwest::blocking::Client;
use serde::Serialize;
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}},
    thread,
    time::Instant,
};

#[derive(Clone, Debug, Serialize)]
struct HookEvent {
    time: String,
    method: String,
    path: String,
    headers: String,
    body: String,
}

#[derive(Clone, Default)]
struct HookState {
    events: Arc<Mutex<Vec<HookEvent>>>,
}

struct App {
    tab: usize,
    method_idx: usize,
    url: String,
    headers: String,
    body: String,
    response: String,
    status: String,
    duration: String,
    sending: bool,
    hook_port: String,
    hook_running: bool,
    hook_running_flag: Option<Arc<AtomicBool>>,
    hook_state: HookState,
    hook_status: String,
    selected_event: Option<usize>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            tab: 0,
            method_idx: 0,
            url: "https://httpbin.org/get".into(),
            headers: "Content-Type: application/json\n".into(),
            body: "{\n  \"hello\": \"world\"\n}".into(),
            response: "Send a request to see the response here.".into(),
            status: "Ready".into(),
            duration: "—".into(),
            sending: false,
            hook_port: "8787".into(),
            hook_running: false,
            hook_running_flag: None,
            hook_state: HookState::default(),
            hook_status: "Webhook listener is stopped.".into(),
            selected_event: None,
        }
    }
}

impl App {
    fn send_request(&mut self) {
        if self.url.trim().is_empty() {
            self.status = "Enter a URL first.".into();
            return;
        }
        let method = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"][self.method_idx];
        let url = self.url.trim().to_owned();
        let headers_text = self.headers.clone();
        let body = self.body.clone();
        self.sending = true;
        let start = Instant::now();

        let client = match Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(concat!("API-Webhook-Tester/", env!("CARGO_PKG_VERSION")))
            .build() {
            Ok(c) => c,
            Err(e) => {
                self.status = format!("Client error: {e}");
                self.sending = false;
                return;
            }
        };

        let mut request = match method {
            "GET" => client.get(&url),
            "POST" => client.post(&url),
            "PUT" => client.put(&url),
            "PATCH" => client.patch(&url),
            "DELETE" => client.delete(&url),
            "HEAD" => client.head(&url),
            _ => client.get(&url),
        };

        for line in headers_text.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                if !key.is_empty() {
                    if let (Ok(k), Ok(v)) = (
                        reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                        reqwest::header::HeaderValue::from_str(value),
                    ) {
                        request = request.header(k, v);
                    }
                }
            }
        }
        if matches!(method, "POST" | "PUT" | "PATCH") && !body.trim().is_empty() {
            request = request.body(body);
        }

        match request.send() {
            Ok(resp) => {
                let code = resp.status();
                let headers_out = format!("{:#?}", resp.headers());
                let text = resp.text().unwrap_or_else(|e| format!("[Could not read response body: {e}]"));
                self.status = format!("{method} • HTTP {code}");
                self.duration = format!("{} ms", start.elapsed().as_millis());
                self.response = format!("Headers:\n{headers_out}\n\nBody:\n{text}");
            }
            Err(e) => {
                self.status = format!("Request failed: {e}");
                self.duration = format!("{} ms", start.elapsed().as_millis());
                self.response = format!("{e:#}");
            }
        }
        self.sending = false;
    }

    fn start_webhook(&mut self) {
        if self.hook_running { return; }
        let port: u16 = match self.hook_port.trim().parse() {
            Ok(p) if p > 0 => p,
            _ => { self.hook_status = "Enter a valid port (1–65535).".into(); return; }
        };
        let state = self.hook_state.clone();
        let running = Arc::new(AtomicBool::new(true));
        let running_thread = running.clone();
        self.hook_running_flag = Some(running);
        thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(_) => return,
            };
            rt.block_on(async move {
                let app = Router::new()
                    .route("/", any(receive_webhook))
                    .route("/*path", any(receive_webhook))
                    .with_state(state);
                let addr = SocketAddr::from(([0, 0, 0, 0], port));
                if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
                    let _ = axum::serve(listener, app).with_graceful_shutdown(async move {
                        while running_thread.load(Ordering::Relaxed) {
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        }
                    }).await;
                }
            });
        });
        self.hook_running = true;
        self.hook_status = format!("Listening on 0.0.0.0:{port}. Send a request to this port.");
    }
}

async fn receive_webhook(
    State(state): State<HookState>,
    method: Method,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let headers_text = headers.iter()
        .map(|(k, v)| format!("{}: {}", k, v.to_str().unwrap_or("<binary>")))
        .collect::<Vec<_>>().join("\n");
    let body_text = String::from_utf8_lossy(&body).to_string();
    let event = HookEvent {
        time: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        method: method.to_string(),
        path: uri.to_string(),
        headers: headers_text,
        body: body_text,
    };
    if let Ok(mut events) = state.events.lock() {
        events.insert(0, event);
        events.truncate(200);
    }
    (axum::http::StatusCode::OK, "Webhook received")
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("⚡ API & Webhook Tester");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("v{}", env!("CARGO_PKG_VERSION")));
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.selectable_label(self.tab == 0, "  API Request  ").clicked() { self.tab = 0; }
                if ui.selectable_label(self.tab == 1, "  Webhook Listener  ").clicked() { self.tab = 1; }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.tab == 0 {
                ui.add_space(8.0);
                ui.label("Send HTTP requests and inspect status, headers, latency, and response body.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("method")
                        .selected_text(["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"][self.method_idx])
                        .show_ui(ui, |ui| {
                            for (i, m) in ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"].iter().enumerate() {
                                ui.selectable_value(&mut self.method_idx, i, *m);
                            }
                        });
                    ui.add(egui::TextEdit::singleline(&mut self.url).hint_text("https://api.example.com/endpoint").desired_width(f32::INFINITY));
                    if ui.add_enabled(!self.sending, egui::Button::new("▶ Send")).clicked() { self.send_request(); }
                });
                ui.columns(2, |cols| {
                    cols[0].label("Headers (one per line: Key: Value)");
                    cols[1].label("Request body");
                    cols[0].add(egui::TextEdit::multiline(&mut self.headers).desired_rows(8).code_editor().desired_width(f32::INFINITY));
                    cols[1].add(egui::TextEdit::multiline(&mut self.body).desired_rows(8).code_editor().desired_width(f32::INFINITY));
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.strong(&self.status);
                    ui.separator();
                    ui.label(format!("Time: {}", self.duration));
                });
                ui.add(egui::TextEdit::multiline(&mut self.response).desired_rows(16).code_editor().desired_width(f32::INFINITY));
            } else {
                ui.add_space(8.0);
                ui.label("Receive webhook callbacks on your computer and inspect incoming requests.");
                ui.horizontal(|ui| {
                    ui.label("Port");
                    ui.add(egui::TextEdit::singleline(&mut self.hook_port).desired_width(100.0));
                    if !self.hook_running && ui.button("▶ Start listener").clicked() { self.start_webhook(); }
                    if self.hook_running && ui.button("■ Stop listener").clicked() {
                        if let Some(flag) = &self.hook_running_flag { flag.store(false, Ordering::Relaxed); }
                        self.hook_running = false;
                        self.hook_status = "Listener stopped.".into();
                    }
                    if self.hook_running { ui.label("● Running"); }
                });
                ui.label(&self.hook_status);
                ui.monospace(format!("Local URL: http://127.0.0.1:{}/", self.hook_port));
                ui.small("For external services, expose this port with a trusted tunnel (for example, a secured development tunnel). Avoid forwarding sensitive payloads.");
                ui.separator();
                let events = self.hook_state.events.lock().map(|e| e.clone()).unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.heading(format!("Incoming requests ({})", events.len()));
                    if ui.button("Clear").clicked() {
                        if let Ok(mut e) = self.hook_state.events.lock() { e.clear(); }
                        self.selected_event = None;
                    }
                });
                egui::ScrollArea::vertical().max_height(170.0).show(ui, |ui| {
                    for (i, event) in events.iter().enumerate() {
                        if ui.selectable_label(self.selected_event == Some(i), format!("{}  {} {}", event.time, event.method, event.path)).clicked() {
                            self.selected_event = Some(i);
                        }
                    }
                });
                if let Some(i) = self.selected_event {
                    if let Some(event) = events.get(i) {
                        ui.separator();
                        ui.label("Headers");
                        let mut selected_headers = event.headers.clone();
                        ui.add(egui::TextEdit::multiline(&mut selected_headers).desired_rows(4).code_editor());
                        ui.label("Body");
                        let mut selected_body = event.body.clone();
                        ui.add(egui::TextEdit::multiline(&mut selected_body).desired_rows(8).code_editor());
                    }
                } else {
                    ui.label("Select an incoming request to inspect its headers and body.");
                }
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 820.0])
            .with_min_inner_size([760.0, 600.0])
            .with_title("API & Webhook Tester"),
        ..Default::default()
    };
    eframe::run_native("API & Webhook Tester", options, Box::new(|_cc| Ok(Box::<App>::default())))
}
