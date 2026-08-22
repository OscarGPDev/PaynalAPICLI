use crate::evaluator::VariableContext;
use crate::manifest::PaynalManifest;
use crate::models::PaynalFile;
use crate::runner::HttpRunner;
use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::collections::HashSet;
use std::fs;
use std::io::stdout;
use std::path::{Path, PathBuf};

// Theme palette extracted from Paynalapicli logo
const AZTEC_TEAL: Color = Color::Rgb(27, 162, 168);
const AZTEC_GOLD: Color = Color::Rgb(245, 166, 35);
const AZTEC_RUST: Color = Color::Rgb(217, 83, 39);
const AZTEC_DARK: Color = Color::Rgb(14, 41, 48);

const VERBS: [&str; 7] = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];
const BODY_TYPES: [&str; 7] = ["json", "form", "multipart", "file", "xml", "text", "none"];

#[derive(Clone, Debug, PartialEq, Eq)]
enum TreeItem {
    Folder { name: String, is_expanded: bool, count: usize },
    File { path: PathBuf, display_name: String },
}

struct App {
    all_files: Vec<PathBuf>,
    visible_items: Vec<TreeItem>,
    list_state: ListState,
    expanded_folders: HashSet<String>,
    search_query: String,
    is_searching: bool,
    is_adding: bool,
    add_input: String,
    add_verb_idx: usize,
    add_body_idx: usize,
    add_is_routine: bool,
    is_confirming_delete: bool,
    is_editing_vars: bool,
    var_input: String,
    is_managing_env: bool,
    env_input: String,
    selected_file_content: Option<PaynalFile>,
    raw_content: String,
    last_response: Option<String>,
    is_executing: bool,
    env_profile: Option<String>,
    _manifest: PaynalManifest,
    runner: HttpRunner,
}

impl App {
    fn new(env_profile: Option<String>) -> Self {
        let manifest = PaynalManifest::load_from_dir(Path::new(".")).unwrap_or_default();
        let runner = HttpRunner::new(manifest.validate_certificates, manifest.proxy.as_deref());

        let mut app = Self {
            all_files: Vec::new(),
            visible_items: Vec::new(),
            list_state: ListState::default(),
            expanded_folders: HashSet::new(),
            search_query: String::new(),
            is_searching: false,
            is_adding: false,
            add_input: String::new(),
            add_verb_idx: 0,
            add_body_idx: 0,
            add_is_routine: false,
            is_confirming_delete: false,
            is_editing_vars: false,
            var_input: String::new(),
            is_managing_env: false,
            env_input: String::new(),
            selected_file_content: None,
            raw_content: String::new(),
            last_response: None,
            is_executing: false,
            env_profile,
            _manifest: manifest,
            runner,
        };

        app.sync_add_body_to_verb();
        app.reload_files();
        app
    }

    fn sync_add_body_to_verb(&mut self) {
        let verb = VERBS[self.add_verb_idx];
        let default_type = self._manifest.get_default_body_type(verb);
        if let Some(idx) = BODY_TYPES.iter().position(|&bt| bt.eq_ignore_ascii_case(&default_type)) {
            self.add_body_idx = idx;
        } else {
            self.add_body_idx = 0;
        }
    }

    fn get_env_filepath(&self) -> PathBuf {
        if let Some(prof) = &self.env_profile {
            PathBuf::from(format!("paynal.env.{}", prof))
        } else {
            PathBuf::from("paynal.env")
        }
    }

    fn open_in_editor(&mut self) -> Result<()> {
        if let Some(index) = self.list_state.selected() {
            if let Some(TreeItem::File { path, .. }) = self.visible_items.get(index) {
                let editor = std::env::var("EDITOR")
                    .or_else(|_| std::env::var("VISUAL"))
                    .unwrap_or_else(|_| "nano".to_string());

                disable_raw_mode()?;
                execute!(stdout(), LeaveAlternateScreen)?;

                let status = std::process::Command::new(&editor)
                    .arg(path)
                    .status();

                enable_raw_mode()?;
                execute!(stdout(), EnterAlternateScreen)?;

                match status {
                    Ok(s) if s.success() => {
                        self.last_response = Some(format!("✏️ Finished editing: {}", path.display()));
                    }
                    Ok(s) => {
                        self.last_response = Some(format!("⚠️ Editor exited with code: {:?}", s.code()));
                    }
                    Err(e) => {
                        self.last_response = Some(format!("❌ Failed to launch editor '{}': {}", editor, e));
                    }
                }

                self.load_selected_file();
            }
        }
        Ok(())
    }

    fn collect_yaml_files(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    files.extend(Self::collect_yaml_files(&path));
                } else if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext == "yaml" || ext == "yml" {
                            files.push(path);
                        }
                    }
                }
            }
        }
        files
    }

    fn reload_files(&mut self) {
        let collections_base = PathBuf::from(&self._manifest.root_dir).join("collections");
        let mut files = if collections_base.exists() {
            Self::collect_yaml_files(&collections_base)
        } else {
            Vec::new()
        };
        files.sort();
        self.all_files = files;

        for f in &self.all_files {
            if let Ok(rel) = f.strip_prefix("collections") {
                if let Some(parent) = rel.parent() {
                    let folder_str = parent.to_string_lossy().to_string();
                    if !folder_str.is_empty() {
                        self.expanded_folders.insert(folder_str);
                    }
                }
            }
        }

        self.rebuild_visible_items();
        if !self.visible_items.is_empty() && self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        }
        self.load_selected_file();
    }

    fn rebuild_visible_items(&mut self) {
        let mut items = Vec::new();

        let filtered_files: Vec<&PathBuf> = self
            .all_files
            .iter()
            .filter(|f| {
                if self.search_query.trim().is_empty() {
                    true
                } else {
                    let path_str = f.to_string_lossy().to_lowercase();
                    path_str.contains(&self.search_query.to_lowercase())
                }
            })
            .collect();

        let mut folder_map: std::collections::BTreeMap<String, Vec<&PathBuf>> = std::collections::BTreeMap::new();
        let mut root_files = Vec::new();

        for f in filtered_files {
            let rel = f.strip_prefix("collections").unwrap_or(f);
            if let Some(parent) = rel.parent() {
                let folder_str = parent.to_string_lossy().to_string();
                if folder_str.is_empty() || folder_str == "." {
                    root_files.push(f);
                } else {
                    folder_map.entry(folder_str).or_default().push(f);
                }
            } else {
                root_files.push(f);
            }
        }

        for (folder, files) in folder_map {
            let is_expanded = self.expanded_folders.contains(&folder) || !self.search_query.is_empty();
            items.push(TreeItem::Folder {
                name: folder.clone(),
                is_expanded,
                count: files.len(),
            });

            if is_expanded {
                for file in files {
                    let display_name = file
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| file.to_string_lossy().to_string());
                    items.push(TreeItem::File {
                        path: file.clone(),
                        display_name,
                    });
                }
            }
        }

        for file in root_files {
            let display_name = file
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| file.to_string_lossy().to_string());
            items.push(TreeItem::File {
                path: file.clone(),
                display_name,
            });
        }

        self.visible_items = items;
    }

    fn load_selected_file(&mut self) {
        if let Some(index) = self.list_state.selected() {
            if let Some(TreeItem::File { path, .. }) = self.visible_items.get(index) {
                if let Ok(content) = fs::read_to_string(path) {
                    self.raw_content = content.clone();
                    self.selected_file_content = serde_yaml::from_str(&content).ok();
                    return;
                }
            }
        }
        self.raw_content = String::new();
        self.selected_file_content = None;
    }

    fn next(&mut self) {
        if self.visible_items.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.visible_items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.load_selected_file();
    }

    fn previous(&mut self) {
        if self.visible_items.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.visible_items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.load_selected_file();
    }

    fn toggle_expand(&mut self) {
        if let Some(index) = self.list_state.selected() {
            if let Some(item) = self.visible_items.get(index).cloned() {
                if let TreeItem::Folder { name, .. } = item {
                    if self.expanded_folders.contains(&name) {
                        self.expanded_folders.remove(&name);
                    } else {
                        self.expanded_folders.insert(name);
                    }
                    self.rebuild_visible_items();
                }
            }
        }
    }

    fn commit_add(&mut self) {
        let input = self.add_input.trim().to_string();
        if !input.is_empty() {
            let verb = VERBS[self.add_verb_idx];
            let body_type = BODY_TYPES[self.add_body_idx];
            let is_routine = self.add_is_routine || input.contains("--routine");
            let clean_path = input.replace("--routine", "").trim().to_string();

            if let Err(e) = crate::commands::add::execute_add(
                clean_path.clone(),
                false,
                verb.to_string(),
                is_routine,
                Some(body_type.to_string()),
            ) {
                self.last_response = Some(format!("❌ Failed to create template: {}", e));
            } else {
                self.last_response = Some(format!(
                    "✨ Created {} template [{}, body: {}] at: collections/{}.yaml",
                    if is_routine { "Routine" } else { "Request" },
                    verb,
                    body_type,
                    clean_path
                ));
                self.reload_files();
            }
        }
        self.is_adding = false;
        self.add_input.clear();
    }

    fn commit_edit_var(&mut self) {
        let input = self.var_input.trim().to_string();
        if !input.is_empty() {
            if let Some(index) = self.list_state.selected() {
                if let Some(TreeItem::File { path, .. }) = self.visible_items.get(index).cloned() {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(mut paynal_file) = serde_yaml::from_str::<PaynalFile>(&content) {
                            if let Some((k, v)) = input.split_once('=') {
                                paynal_file.vars.insert(k.trim().to_string(), v.trim().to_string());
                                if let Ok(new_yaml) = serde_yaml::to_string(&paynal_file) {
                                    if fs::write(&path, new_yaml).is_ok() {
                                        self.last_response = Some(format!("✏️ Variable set: {} = {}", k.trim(), v.trim()));
                                        self.load_selected_file();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        self.is_editing_vars = false;
        self.var_input.clear();
    }

    fn commit_edit_env(&mut self) {
        let input = self.env_input.trim().to_string();
        let env_file = self.get_env_filepath();

        if !input.is_empty() {
            if let Some((k, v)) = input.split_once('=') {
                let key = k.trim();
                let val = v.trim();

                let mut lines = Vec::new();
                if env_file.exists() {
                    if let Ok(content) = fs::read_to_string(&env_file) {
                        lines = content.lines().map(|s| s.to_string()).collect();
                    }
                }

                let mut updated = false;
                for line in &mut lines {
                    if line.starts_with(&format!("{}=", key)) || line.starts_with(&format!("export {}=", key)) {
                        *line = format!("{}={}", key, val);
                        updated = true;
                        break;
                    }
                }

                if !updated {
                    lines.push(format!("{}={}", key, val));
                }

                if fs::write(&env_file, lines.join("\n") + "\n").is_ok() {
                    self.last_response = Some(format!("🔒 Environment Variable Saved to {}: {}={}", env_file.display(), key, val));
                }
            }
        }
        self.is_managing_env = false;
        self.env_input.clear();
    }

    fn display_current_env(&mut self) {
        let env_file = self.get_env_filepath();
        let mut content = format!("🔒 Environment Configuration File ({})\n────────────────────────────────────────\n", env_file.display());

        if env_file.exists() {
            if let Ok(raw) = fs::read_to_string(&env_file) {
                content.push_str(&raw);
            } else {
                content.push_str("(empty)");
            }
        } else {
            content.push_str("(file does not exist yet)");
        }
        self.last_response = Some(content);
    }

    fn confirm_delete(&mut self) {
        if let Some(index) = self.list_state.selected() {
            if let Some(TreeItem::File { path, .. }) = self.visible_items.get(index).cloned() {
                if let Err(e) = fs::remove_file(&path) {
                    self.last_response = Some(format!("❌ Failed to delete file: {}", e));
                } else {
                    self.last_response = Some(format!("🗑️ Deleted file: {}", path.display()));
                    self.reload_files();
                }
            }
        }
        self.is_confirming_delete = false;
    }

    async fn run_selected_or_collection(&mut self, parallel: bool) {
        if let Some(index) = self.list_state.selected() {
            if let Some(item) = self.visible_items.get(index).cloned() {
                match item {
                    TreeItem::Folder { name, .. } => {
                        self.run_folder_collection(&name, parallel).await;
                    }
                    TreeItem::File { .. } => {
                        self.run_single_file().await;
                    }
                }
            }
        }
    }

    async fn run_folder_collection(&mut self, folder_name: &str, parallel: bool) {
        self.is_executing = true;
        let mode_str = if parallel { "Parallel [CPUMAX]" } else { "Sequential" };
        self.last_response = Some(format!("⚡ Executing Collection '{}' ({})...", folder_name, mode_str));

        let target_dir = PathBuf::from("collections").join(folder_name);
        let target_files: Vec<PathBuf> = self
            .all_files
            .iter()
            .filter(|f| f.starts_with(&target_dir) || f.starts_with(Path::new(".").join(&target_dir)))
            .cloned()
            .collect();

        if target_files.is_empty() {
            self.last_response = Some(format!("⚠️ No files found in collection folder: {}", folder_name));
            self.is_executing = false;
            return;
        }

        let mut output = Vec::new();
        output.push(format!("📦 Executing Collection: {} ({} files | {})", folder_name, target_files.len(), mode_str));
        output.push("────────────────────────────────────────".to_string());

        let mut passed = 0;
        let mut failed = 0;

        for file_path in target_files {
            let rel_name = file_path
                .strip_prefix("collections")
                .unwrap_or(&file_path)
                .display()
                .to_string();

            if let Ok(content) = fs::read_to_string(&file_path) {
                if let Ok(paynal_file) = serde_yaml::from_str::<PaynalFile>(&content) {
                    let mut ctx = VariableContext::new_with_env(self.env_profile.as_deref());
                    ctx.extend(&paynal_file.vars);

                    if paynal_file.is_routine() {
                        output.push(format!("🔄 Routine: {}", rel_name));
                        for step in &paynal_file.steps {
                            let step_name = step.name.as_deref().unwrap_or(&step.id);
                            let url = ctx.interpolate(&step.request.url);

                            match self.runner.execute(&step.request, step.assert.as_ref(), &ctx).await {
                                Ok(res) => {
                                    output.push(format!("  ✔ Step: {} [{} {}] → Status {} ({}ms)", step_name, step.request.method, url, res.status, res.duration_ms));
                                    self.runner.extract_captures(&res, &step.capture, &mut ctx);
                                    passed += 1;
                                }
                                Err(e) => {
                                    output.push(format!("  ❌ Step {} Failed: {}", step_name, e));
                                    failed += 1;
                                    break;
                                }
                            }
                        }
                    } else if let Some(req) = &paynal_file.request {
                        let url = ctx.interpolate(&req.url);
                        match self.runner.execute(req, paynal_file.assert.as_ref(), &ctx).await {
                            Ok(res) => {
                                output.push(format!("  ✔ Request: {} [{} {}] → Status {} ({}ms)", rel_name, req.method, url, res.status, res.duration_ms));
                                passed += 1;
                            }
                            Err(e) => {
                                output.push(format!("  ❌ Request {} Failed: {}", rel_name, e));
                                failed += 1;
                            }
                        }
                    }
                }
            }
        }

        output.push("────────────────────────────────────────".to_string());
        output.push(format!("🏁 Collection Results: {} Passed | {} Failed", passed, failed));
        self.last_response = Some(output.join("\n"));
        self.is_executing = false;
    }

    async fn run_single_file(&mut self) {
        self.is_executing = true;
        self.last_response = Some("⏳ Executing request...".to_string());

        let mut ctx = VariableContext::new_with_env(self.env_profile.as_deref());
        if let Some(pf) = &self.selected_file_content {
            ctx.extend(&pf.vars);
        }

        let mut output = Vec::new();

        if let Some(paynal_file) = &self.selected_file_content {
            if paynal_file.is_routine() {
                output.push(format!("🔄 Routine: {}", paynal_file.name));
                for step in &paynal_file.steps {
                    let step_name = step.name.as_deref().unwrap_or(&step.id);
                    let url = ctx.interpolate(&step.request.url);

                    let mut payload_desc = String::new();
                    if let Some(form) = &step.request.form_data {
                        let form_lines: Vec<String> = form.iter().map(|(k, v)| format!("    {}: {}", k, v)).collect();
                        payload_desc = format!("\n  Multipart Form-Data:\n{}\n", form_lines.join("\n"));
                    } else if let Some(body) = &step.request.body {
                        if !body.trim().is_empty() {
                            payload_desc = format!("\n  Payload Body:\n    {}\n", body.trim());
                        }
                    }

                    match self.runner.execute(&step.request, step.assert.as_ref(), &ctx).await {
                        Ok(res) => {
                            output.push(format!(
                                "▶ Step: {} [{} {}]\n  Status: {} | Time: {}ms{}\n  Response Body:\n{}\n",
                                step_name, step.request.method, url, res.status, res.duration_ms, payload_desc, res.body
                            ));
                            self.runner.extract_captures(&res, &step.capture, &mut ctx);
                        }
                        Err(e) => {
                            output.push(format!("❌ Step {} Failed: {}", step_name, e));
                            break;
                        }
                    }
                }
            } else if let Some(req) = &paynal_file.request {
                let url = ctx.interpolate(&req.url);

                let mut payload_desc = String::new();
                if let Some(form) = &req.form_data {
                    let form_lines: Vec<String> = form.iter().map(|(k, v)| format!("    {}: {}", k, v)).collect();
                    payload_desc = format!("\nMultipart Form-Data:\n{}\n", form_lines.join("\n"));
                } else if let Some(body) = &req.body {
                    if !body.trim().is_empty() {
                        payload_desc = format!("\nPayload Body:\n{}\n", body.trim());
                    }
                }

                match self.runner.execute(req, paynal_file.assert.as_ref(), &ctx).await {
                    Ok(res) => {
                        output.push(format!(
                            "🚀 Request: {} [{} {}]\nStatus: {} | Latency: {}ms{}\n\nResponse Headers:\n{}\n\nResponse Body:\n{}",
                            paynal_file.name, req.method, url, res.status, res.duration_ms,
                            payload_desc,
                            res.headers.iter().map(|(k, v)| format!("{}: {}", k, v.to_str().unwrap_or(""))).collect::<Vec<_>>().join("\n"),
                            res.body
                        ));
                    }
                    Err(e) => {
                        output.push(format!("❌ Request Failed: {}", e));
                    }
                }
            }
        }

        self.last_response = Some(output.join("\n────────────────────────────────────────\n"));
        self.is_executing = false;
    }
}

pub async fn execute_tui(env_profile: Option<String>) -> Result<()> {
    enable_raw_mode().context("Failed to enable terminal raw mode")?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(env_profile);

    let res = run_app(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("TUI Error: {:?}", err);
    }

    Ok(())
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if app.is_adding {
                        match key.code {
                            KeyCode::Esc => {
                                app.is_adding = false;
                                app.add_input.clear();
                            }
                            KeyCode::Tab => {
                                app.add_verb_idx = (app.add_verb_idx + 1) % VERBS.len();
                                app.sync_add_body_to_verb();
                            }
                            KeyCode::Char('b') | KeyCode::Char('B') => {
                                app.add_body_idx = (app.add_body_idx + 1) % BODY_TYPES.len();
                            }
                            KeyCode::Char('!') => {
                                app.add_is_routine = !app.add_is_routine;
                            }
                            KeyCode::Enter => {
                                app.commit_add();
                            }
                            KeyCode::Backspace => {
                                app.add_input.pop();
                            }
                            KeyCode::Char(c) => {
                                app.add_input.push(c);
                            }
                            _ => {}
                        }
                    } else if app.is_editing_vars {
                        match key.code {
                            KeyCode::Esc => {
                                app.is_editing_vars = false;
                                app.var_input.clear();
                            }
                            KeyCode::Enter => {
                                app.commit_edit_var();
                            }
                            KeyCode::Backspace => {
                                app.var_input.pop();
                            }
                            KeyCode::Char(c) => {
                                app.var_input.push(c);
                            }
                            _ => {}
                        }
                    } else if app.is_managing_env {
                        match key.code {
                            KeyCode::Esc => {
                                app.is_managing_env = false;
                                app.env_input.clear();
                            }
                            KeyCode::Enter => {
                                app.commit_edit_env();
                            }
                            KeyCode::Backspace => {
                                app.env_input.pop();
                            }
                            KeyCode::Char(c) => {
                                app.env_input.push(c);
                            }
                            _ => {}
                        }
                    } else if app.is_confirming_delete {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                app.confirm_delete();
                            }
                            _ => {
                                app.is_confirming_delete = false;
                            }
                        }
                    } else if app.is_searching {
                        match key.code {
                            KeyCode::Esc | KeyCode::Enter => {
                                app.is_searching = false;
                            }
                            KeyCode::Backspace => {
                                app.search_query.pop();
                                app.rebuild_visible_items();
                                app.list_state.select(Some(0));
                                app.load_selected_file();
                            }
                            KeyCode::Char(c) => {
                                app.search_query.push(c);
                                app.rebuild_visible_items();
                                app.list_state.select(Some(0));
                                app.load_selected_file();
                            }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                            KeyCode::Char('a') => {
                                app.is_adding = true;
                                app.add_input.clear();
                                app.add_verb_idx = 0;
                                app.add_is_routine = false;
                                app.sync_add_body_to_verb();
                            }
                            KeyCode::Char('e') => {
                                let _ = app.open_in_editor();
                            }
                            KeyCode::Char('v') => {
                                if let Some(index) = app.list_state.selected() {
                                    if let Some(TreeItem::File { .. }) = app.visible_items.get(index) {
                                        app.is_editing_vars = true;
                                        app.var_input.clear();
                                    }
                                }
                            }
                            KeyCode::Char('E') => {
                                app.is_managing_env = true;
                                app.env_input.clear();
                                app.display_current_env();
                            }
                            KeyCode::Char('d') | KeyCode::Delete => {
                                if let Some(index) = app.list_state.selected() {
                                    if let Some(TreeItem::File { .. }) = app.visible_items.get(index) {
                                        app.is_confirming_delete = true;
                                    }
                                }
                            }
                            KeyCode::Char('/') => {
                                app.is_searching = true;
                            }
                            KeyCode::Char('o') | KeyCode::Right | KeyCode::Left => {
                                app.toggle_expand();
                            }
                            KeyCode::Char('p') => {
                                app.run_selected_or_collection(true).await;
                            }
                            KeyCode::Down | KeyCode::Char('j') => app.next(),
                            KeyCode::Up | KeyCode::Char('k') => app.previous(),
                            KeyCode::Enter | KeyCode::Char('r') | KeyCode::Char(' ') => {
                                app.run_selected_or_collection(false).await;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Banner / Header
            Constraint::Min(10),   // Main content
            Constraint::Length(3), // Footer / Shortcuts / Prompt
        ])
        .split(f.area());

    // 1. Header Banner
    let env_str = app.env_profile.as_deref().unwrap_or("default");
    let header_text = vec![Line::from(vec![
        Span::styled("⚡ PAYNALAPICLI ", Style::default().fg(AZTEC_TEAL).add_modifier(Modifier::BOLD)),
        Span::styled("— CLI API Dashboard ", Style::default().fg(AZTEC_GOLD)),
        Span::styled(format!("[Env: {}] ", env_str), Style::default().fg(AZTEC_RUST).add_modifier(Modifier::BOLD)),
        Span::styled(format!("({} total / {} visible)", app.all_files.len(), app.visible_items.len()), Style::default().fg(Color::Gray)),
    ])];
    let header = Paragraph::new(header_text)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(AZTEC_TEAL)).style(Style::default().bg(AZTEC_DARK)));
    f.render_widget(header, chunks[0]);

    // 2. Main Content (Left: Collections Tree, Center: YAML Preview, Right: Response)
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(28), // Collections Tree & Search
            Constraint::Percentage(34), // YAML / Request Definition
            Constraint::Percentage(38), // Output Response
        ])
        .split(chunks[1]);

    // Left: Collections Tree Items
    let items: Vec<ListItem> = app
        .visible_items
        .iter()
        .map(|item| match item {
            TreeItem::Folder { name, is_expanded, count } => {
                let icon = if *is_expanded { "▼ 📁" } else { "▶ 📁" };
                ListItem::new(Span::styled(
                    format!("{} {} ({})", icon, name, count),
                    Style::default().fg(AZTEC_GOLD).add_modifier(Modifier::BOLD),
                ))
            }
            TreeItem::File { display_name, .. } => ListItem::new(Span::styled(
                format!("  📜 {}", display_name),
                Style::default().fg(Color::White),
            )),
        })
        .collect();

    let list_title = if app.is_searching {
        format!(" 📁 Search: {} ", app.search_query)
    } else if !app.search_query.is_empty() {
        format!(" 📁 Collections [Filter: {}] ", app.search_query)
    } else {
        " 📁 Collections Tree ".to_string()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title(list_title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if app.is_searching { AZTEC_GOLD } else { AZTEC_TEAL })),
        )
        .highlight_style(
            Style::default()
                .bg(AZTEC_TEAL)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, main_chunks[0], &mut app.list_state);

    // Center: Request YAML Definition
    let yaml_title = match &app.selected_file_content {
        Some(pf) => format!(" 📜 YAML: {} ", pf.name),
        None => " 📜 YAML Definition ".to_string(),
    };
    let yaml_widget = Paragraph::new(app.raw_content.as_str())
        .block(Block::default().title(yaml_title).borders(Borders::ALL).border_style(Style::default().fg(AZTEC_GOLD)))
        .wrap(Wrap { trim: false });
    f.render_widget(yaml_widget, main_chunks[1]);

    // Right: Execution Log / Response Output
    let resp_text = app
        .last_response
        .as_deref()
        .unwrap_or("Press [Enter] or [Space] or [r] to run selected request/routine.\nPress [p] on a Folder to run entire Collection in Parallel!");
    let resp_title = if app.is_executing {
        " ⏳ Executing... "
    } else if app.is_managing_env {
        " 🔒 Environment Configuration "
    } else {
        " 📡 Response Output "
    };
    let response_widget = Paragraph::new(resp_text)
        .block(Block::default().title(resp_title).borders(Borders::ALL).border_style(Style::default().fg(AZTEC_RUST)))
        .wrap(Wrap { trim: false });
    f.render_widget(response_widget, main_chunks[2]);

    // 3. Footer Shortcuts / Search status / Add & Delete Prompts
    let footer_text = if app.is_adding {
        let current_verb = VERBS[app.add_verb_idx];
        let current_body = BODY_TYPES[app.add_body_idx];
        let type_label = if app.add_is_routine { "Routine 🔄" } else { "Request 🚀" };
        vec![Line::from(vec![
            Span::styled(" ➕ ADD TEMPLATE: ", Style::default().fg(AZTEC_GOLD).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{}_ ", app.add_input), Style::default().fg(Color::White)),
            Span::styled(" [Tab] ", Style::default().fg(AZTEC_TEAL).add_modifier(Modifier::BOLD)),
            Span::styled(format!("Verb: {}  ", current_verb), Style::default().fg(AZTEC_GOLD)),
            Span::styled(" [b] ", Style::default().fg(AZTEC_TEAL).add_modifier(Modifier::BOLD)),
            Span::styled(format!("Body: {}  ", current_body), Style::default().fg(AZTEC_GOLD)),
            Span::styled(" [!] ", Style::default().fg(AZTEC_TEAL).add_modifier(Modifier::BOLD)),
            Span::styled(format!("Type: {}  ", type_label), Style::default().fg(AZTEC_RUST)),
            Span::styled("[Enter] ", Style::default().fg(AZTEC_TEAL)),
            Span::raw("Create  "),
            Span::styled("[Esc] ", Style::default().fg(AZTEC_RUST)),
            Span::raw("Cancel"),
        ])]
    } else if app.is_editing_vars {
        vec![Line::from(vec![
            Span::styled(" ✏️ EDIT FILE VARS: ", Style::default().fg(AZTEC_TEAL).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{}_ ", app.var_input), Style::default().fg(Color::White)),
            Span::styled(" (format: key=value) [Enter] Set Var  [Esc] Done ", Style::default().fg(Color::Gray)),
        ])]
    } else if app.is_managing_env {
        let env_name = app.get_env_filepath().display().to_string();
        vec![Line::from(vec![
            Span::styled(format!(" 🔒 MANAGE ENV [{}]: ", env_name), Style::default().fg(AZTEC_RUST).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{}_ ", app.env_input), Style::default().fg(Color::White)),
            Span::styled(" (format: KEY=VALUE) [Enter] Save Key  [Esc] Done ", Style::default().fg(Color::Gray)),
        ])]
    } else if app.is_confirming_delete {
        let sel_name = match app.list_state.selected().and_then(|i| app.visible_items.get(i)) {
            Some(TreeItem::File { display_name, .. }) => display_name.as_str(),
            _ => "selected item",
        };
        vec![Line::from(vec![
            Span::styled(format!(" ⚠️ Confirm Delete '{}'? ", sel_name), Style::default().fg(AZTEC_RUST).add_modifier(Modifier::BOLD)),
            Span::styled(" [Y] Yes  [N/Esc] Cancel ", Style::default().fg(AZTEC_GOLD).add_modifier(Modifier::BOLD)),
        ])]
    } else if app.is_searching {
        vec![Line::from(vec![
            Span::styled(" SEARCH MODE: ", Style::default().fg(AZTEC_GOLD).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{}_ ", app.search_query), Style::default().fg(Color::White)),
            Span::styled(" [Enter/Esc] ", Style::default().fg(AZTEC_TEAL)),
            Span::raw("Done searching"),
        ])]
    } else {
        vec![Line::from(vec![
            Span::styled(" [a] ", Style::default().fg(AZTEC_TEAL).add_modifier(Modifier::BOLD)),
            Span::raw("Add  "),
            Span::styled(" [e] ", Style::default().fg(AZTEC_GOLD).add_modifier(Modifier::BOLD)),
            Span::raw("Edit File  "),
            Span::styled(" [v] ", Style::default().fg(AZTEC_GOLD).add_modifier(Modifier::BOLD)),
            Span::raw("Vars  "),
            Span::styled(" [E] ", Style::default().fg(AZTEC_RUST).add_modifier(Modifier::BOLD)),
            Span::raw("Env  "),
            Span::styled(" [d] ", Style::default().fg(AZTEC_RUST).add_modifier(Modifier::BOLD)),
            Span::raw("Del  "),
            Span::styled(" [/] ", Style::default().fg(AZTEC_GOLD).add_modifier(Modifier::BOLD)),
            Span::raw("Search  "),
            Span::styled(" [Enter] ", Style::default().fg(AZTEC_TEAL).add_modifier(Modifier::BOLD)),
            Span::raw("Run  "),
            Span::styled(" [p] ", Style::default().fg(AZTEC_GOLD).add_modifier(Modifier::BOLD)),
            Span::raw("Parallel  "),
            Span::styled(" [q/Esc] ", Style::default().fg(AZTEC_RUST).add_modifier(Modifier::BOLD)),
            Span::raw("Quit"),
        ])]
    };

    let border_color = if app.is_adding || app.is_searching || app.is_editing_vars {
        AZTEC_GOLD
    } else if app.is_confirming_delete || app.is_managing_env {
        AZTEC_RUST
    } else {
        Color::DarkGray
    };

    let footer = Paragraph::new(footer_text)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(border_color)));
    f.render_widget(footer, chunks[2]);
}
