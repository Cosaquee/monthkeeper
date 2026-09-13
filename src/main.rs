use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use chrono::{Datelike, DateTime, Local};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, Write};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect, Alignment},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Row, Table, Clear},
    Terminal,
};
use serde::{Serialize, Deserialize};

const DATA_FILE: &str = "expenses_data.json";
const LOG_FILE: &str = "expenses.log";

fn log_message(message: &str) {
    if let Some(mut data_dir) = dirs::data_dir() {
        data_dir.push("recurring_costs");
        let _ = fs::create_dir_all(&data_dir);
        data_dir.push(LOG_FILE);

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&data_dir)
        {
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(file, "[{}] {}", timestamp, message);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BalanceLog {
    timestamp: DateTime<Local>,
    balance: f64,
    total_expenses: f64,
    unpaid_expenses: f64,
    remaining: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
enum Category {
    Need,
    Want,
}

impl Default for Category {
    fn default() -> Self {
        Category::Need
    }
}

impl Category {
    fn toggle(self) -> Self {
        match self {
            Category::Need => Category::Want,
            Category::Want => Category::Need,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expense {
    description: String,
    amount: f64,
    is_paid: bool,
    #[serde(default)]
    category: Category,
}

impl Expense {
    pub fn new(description: String, amount: f64, category: Category) -> Self {
        Self {
            description,
            amount,
            is_paid: false,
            category,
        }
    }

    pub fn toggle_paid(&mut self) {
        self.is_paid = !self.is_paid;
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExpenseManager {
    expenses: HashMap<String, Vec<Expense>>,
    balances: HashMap<String, f64>,
    balance_history: HashMap<String, Vec<BalanceLog>>,
}

impl ExpenseManager {
    pub fn new() -> Self {
        match Self::load() {
            Ok(manager) => {
                log_message("Successfully loaded existing data");
                manager
            }
            Err(e) => {
                log_message(&format!("Failed to load data: {}. Starting fresh.", e));
                Self {
                    expenses: HashMap::new(),
                    balances: HashMap::new(),
                    balance_history: HashMap::new(),
                }
            }
        }
    }

    fn get_key(year: i32, month: u32) -> String {
        format!("{}-{:02}", year, month)
    }

    fn get_data_file_path() -> PathBuf {
        let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("recurring_costs");
        if let Err(e) = fs::create_dir_all(&path) {
            log_message(&format!("Failed to create data directory: {}", e));
        }
        path.push(DATA_FILE);
        log_message(&format!("Data file path: {}", path.display()));
        path
    }

    pub fn save(&self) -> io::Result<()> {
        let path = Self::get_data_file_path();
        let json = serde_json::to_string_pretty(self)?;
        log_message(&format!("Saving data to: {}", path.display()));
        match fs::write(&path, &json) {
            Ok(_) => {
                log_message("Successfully saved data");
                Ok(())
            }
            Err(e) => {
                log_message(&format!("Failed to save data: {}", e));
                Err(e)
            }
        }
    }

    fn load() -> io::Result<Self> {
        let path = Self::get_data_file_path();
        log_message(&format!("Loading data from: {}", path.display()));
        let json = fs::read_to_string(&path)?;
        match serde_json::from_str(&json) {
            Ok(manager) => Ok(manager),
            Err(e) => {
                log_message(&format!("Failed to parse data: {}", e));
                Err(io::Error::new(io::ErrorKind::InvalidData, e))
            }
        }
    }

    pub fn set_balance(&mut self, balance: f64, year: i32, month: u32) {
        let key = Self::get_key(year, month);
        self.balances.insert(key.clone(), balance);

        let total = self.get_month_total(year, month);
        let unpaid = self.get_unpaid_total(year, month);
        let remaining = balance - unpaid;

        self.balance_history
            .entry(key)
            .or_insert_with(Vec::new)
            .push(BalanceLog {
                timestamp: Local::now(),
                balance,
                total_expenses: total,
                unpaid_expenses: unpaid,
                remaining,
            });

        if let Err(e) = self.save() {
            log_message(&format!("Failed to save after setting balance: {}", e));
        }
    }

    pub fn add_expense(&mut self, year: i32, month: u32, expense: Expense) {
        let key = Self::get_key(year, month);
        self.expenses
            .entry(key)
            .or_insert_with(Vec::new)
            .push(expense);

        if let Err(e) = self.save() {
            log_message(&format!("Failed to save after adding expense: {}", e));
        }
    }

    pub fn get_balance(&self, year: i32, month: u32) -> f64 {
        let key = Self::get_key(year, month);
        self.balances.get(&key).copied().unwrap_or(0.0)
    }

    pub fn get_balance_history(&self, year: i32, month: u32) -> &Vec<BalanceLog> {
        let key = Self::get_key(year, month);
        static EMPTY: Vec<BalanceLog> = Vec::new();
        self.balance_history.get(&key).unwrap_or(&EMPTY)
    }

    pub fn get_month_expenses(&self, year: i32, month: u32) -> Option<&Vec<Expense>> {
        let key = Self::get_key(year, month);
        self.expenses.get(&key)
    }

    pub fn get_month_expenses_mut(&mut self, year: i32, month: u32) -> Option<&mut Vec<Expense>> {
        let key = Self::get_key(year, month);
        self.expenses.get_mut(&key)
    }

    pub fn get_needs_total(&self, year: i32, month: u32) -> f64 {
        let key = Self::get_key(year, month);
        self.expenses
            .get(&key)
            .map(|expenses| {
                expenses.iter()
                    .filter(|e| e.category == Category::Need)
                    .map(|e| e.amount)
                    .sum()
            })
            .unwrap_or(0.0)
    }

    pub fn get_wants_total(&self, year: i32, month: u32) -> f64 {
        let key = Self::get_key(year, month);
        self.expenses
            .get(&key)
            .map(|expenses| {
                expenses.iter()
                    .filter(|e| e.category == Category::Want)
                    .map(|e| e.amount)
                    .sum()
            })
            .unwrap_or(0.0)
    }

    pub fn get_month_total(&self, year: i32, month: u32) -> f64 {
        let key = Self::get_key(year, month);
        self.expenses
            .get(&key)
            .map(|expenses| expenses.iter().map(|e| e.amount).sum())
            .unwrap_or(0.0)
    }

    pub fn get_unpaid_total(&self, year: i32, month: u32) -> f64 {
        let key = Self::get_key(year, month);
        self.expenses
            .get(&key)
            .map(|expenses| {
                expenses.iter()
                    .filter(|e| !e.is_paid)
                    .map(|e| e.amount)
                    .sum()
            })
            .unwrap_or(0.0)
    }

    pub fn delete_expense(&mut self, year: i32, month: u32, index: usize) -> bool {
        let key = Self::get_key(year, month);
        if let Some(expenses) = self.expenses.get_mut(&key) {
            if index < expenses.len() {
                expenses.remove(index);
                if let Err(e) = self.save() {
                    log_message(&format!("Failed to save after deleting expense: {}", e));
                }
                return true;
            }
        }
        false
    }

    pub fn copy_expense_to_next_month(&mut self, year: i32, month: u32, index: usize) -> bool {
        let key = Self::get_key(year, month);
        let expense_to_copy = if let Some(expenses) = self.expenses.get(&key) {
            expenses.get(index).cloned()
        } else {
            None
        };

        if let Some(expense) = expense_to_copy {
            let (next_year, next_month) = if month == 12 {
                (year + 1, 1)
            } else {
                (year, month + 1)
            };

            let mut copied_expense = expense.clone();
            copied_expense.is_paid = false;

            self.add_expense(next_year, next_month, copied_expense);
            log_message(&format!("Copied expense '{}' to {}-{:02}", expense.description, next_year, next_month));
            return true;
        }
        false
    }
}

fn sorted_expenses_grouped(expenses: &[Expense]) -> Vec<(usize, &Expense)> {
    let mut needs: Vec<(usize, &Expense)> = expenses.iter().enumerate()
        .filter(|(_, e)| e.category == Category::Need)
        .collect();
    needs.sort_by(|a, b| a.1.description.to_lowercase().cmp(&b.1.description.to_lowercase()));

    let mut wants: Vec<(usize, &Expense)> = expenses.iter().enumerate()
        .filter(|(_, e)| e.category == Category::Want)
        .collect();
    wants.sort_by(|a, b| a.1.description.to_lowercase().cmp(&b.1.description.to_lowercase()));

    needs.into_iter().chain(wants.into_iter()).collect()
}

fn render_bar(percent: f64, width: usize) -> String {
    let filled = ((percent / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

#[derive(Copy, Clone)]
enum InputMode {
    Normal,
    Adding(InputField),
}

#[derive(Copy, Clone)]
enum InputField {
    Description,
    Amount,
    Category,
}

#[derive(Copy, Clone)]
enum AppMode {
    List,
    Overview,
    History,
    AddBalance,
    ConfirmDelete,
}

struct App {
    expense_manager: ExpenseManager,
    input_mode: InputMode,
    app_mode: AppMode,
    current_year: i32,
    current_month: u32,
    selected_expense: Option<usize>,
    input_description: String,
    input_amount: String,
    input_balance: String,
    input_category: Category,
    pending_delete_index: Option<usize>,
    pending_delete_description: String,
}

impl App {
    fn new() -> Self {
        let now = chrono::Local::now();
        Self {
            expense_manager: ExpenseManager::new(),
            input_mode: InputMode::Normal,
            app_mode: AppMode::List,
            current_year: now.year(),
            current_month: now.month(),
            selected_expense: None,
            input_description: String::new(),
            input_amount: String::new(),
            input_balance: String::new(),
            input_category: Category::Need,
            pending_delete_index: None,
            pending_delete_description: String::new(),
        }
    }
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, mut app: App) -> io::Result<()> {
    loop {
        terminal.draw(|f| {
            match app.app_mode {
                AppMode::List => {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .margin(1)
                        .constraints([
                            Constraint::Length(3),
                            Constraint::Min(0),
                            Constraint::Length(3),
                        ].as_ref())
                        .split(f.size());

                    // Title: centred month + nav hints
                    let month_names = ["January", "February", "March", "April", "May", "June",
                                     "July", "August", "September", "October", "November", "December"];
                    let month_name = month_names[app.current_month as usize - 1];
                    let title_text = format!("◄  {} {}  ►", month_name, app.current_year);
                    let title = Paragraph::new(title_text)
                        .block(Block::default().borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Cyan)))
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
                    f.render_widget(title, chunks[0]);

                    // Main area: expense list + sidebar
                    let content = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([
                            Constraint::Percentage(65),
                            Constraint::Percentage(35),
                        ].as_ref())
                        .split(chunks[1]);

                    // ── Expense table ──────────────────────────────────────────

                    let empty_vec = Vec::new();
                    let expenses = app.expense_manager
                        .get_month_expenses(app.current_year, app.current_month)
                        .unwrap_or(&empty_vec);
                    let sorted = sorted_expenses_grouped(expenses);
                    let n_needs = sorted.iter().filter(|(_, e)| e.category == Category::Need).count();

                    let dim = Style::default().fg(Color::DarkGray);
                    let selected_style = Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD);

                    let mut rows: Vec<Row> = Vec::new();

                    // Needs section
                    rows.push(Row::new(vec!["▸ NEEDS", "", ""])
                        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));
                    for (i, (_, expense)) in sorted[..n_needs].iter().enumerate() {
                        let selected = Some(i) == app.selected_expense;
                        let base = if selected {
                            selected_style
                        } else if expense.is_paid {
                            dim
                        } else {
                            Style::default()
                        };
                        let prefix = if selected { "▶ " } else { "  " };
                        let status = if expense.is_paid { "✓  paid" } else { "✗  due " };
                        rows.push(Row::new(vec![
                            format!("{}{}", prefix, expense.description),
                            format!("{:.2}", expense.amount),
                            status.to_string(),
                        ]).style(base));
                    }

                    // Wants section
                    rows.push(Row::new(vec!["▸ WANTS", "", ""])
                        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
                    for (i, (_, expense)) in sorted[n_needs..].iter().enumerate() {
                        let selected = app.selected_expense == Some(i + n_needs);
                        let base = if selected {
                            selected_style
                        } else if expense.is_paid {
                            dim
                        } else {
                            Style::default()
                        };
                        let prefix = if selected { "▶ " } else { "  " };
                        let status = if expense.is_paid { "✓  paid" } else { "✗  due " };
                        rows.push(Row::new(vec![
                            format!("{}{}", prefix, expense.description),
                            format!("{:.2}", expense.amount),
                            status.to_string(),
                        ]).style(base));
                    }

                    let table = Table::new(rows)
                        .header(
                            Row::new(vec!["  Expense", "Amount", "Status"])
                                .style(dim)
                        )
                        .block(Block::default().borders(Borders::ALL).title(" Expenses "))
                        .widths(&[
                            Constraint::Percentage(52),
                            Constraint::Percentage(24),
                            Constraint::Percentage(24),
                        ]);
                    f.render_widget(table, content[0]);

                    // ── Sidebar ────────────────────────────────────────────────

                    let total = app.expense_manager.get_month_total(app.current_year, app.current_month);
                    let needs_total = app.expense_manager.get_needs_total(app.current_year, app.current_month);
                    let wants_total = app.expense_manager.get_wants_total(app.current_year, app.current_month);
                    let unpaid = app.expense_manager.get_unpaid_total(app.current_year, app.current_month);
                    let paid = total - unpaid;
                    let balance = app.expense_manager.get_balance(app.current_year, app.current_month);
                    let free_money = balance - unpaid;
                    let payment_progress = if total > 0.0 { (paid / total) * 100.0 } else { 100.0 };
                    let needs_pct = if total > 0.0 { (needs_total / total) * 100.0 } else { 0.0 };
                    let wants_pct = if total > 0.0 { (wants_total / total) * 100.0 } else { 0.0 };

                    let bar_w = 16usize;
                    let free_color = if free_money >= 0.0 { Color::Green } else { Color::Red };
                    let progress_color = if payment_progress >= 80.0 { Color::Green }
                        else if payment_progress >= 50.0 { Color::Yellow }
                        else { Color::Red };

                    let label = |s: &str| -> Line {
                        Line::from(Span::styled(s.to_string(), dim))
                    };
                    let divider = || -> Line {
                        Line::from(Span::styled("─────────────────────".to_string(), dim))
                    };
                    let blank = || -> Line { Line::from("") };

                    let sidebar_text = Text::from(vec![
                        label("Balance"),
                        Line::from(Span::styled(
                            format!("{:.2} PLN", balance),
                            Style::default().add_modifier(Modifier::BOLD),
                        )),
                        blank(),
                        label("Free Money"),
                        Line::from(Span::styled(
                            format!("{:.2} PLN", free_money),
                            Style::default().fg(free_color).add_modifier(Modifier::BOLD),
                        )),
                        blank(),
                        divider(),
                        blank(),
                        Line::from(Span::styled(
                            "Needs",
                            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                        )),
                        Line::from(format!("{:.2} PLN", needs_total)),
                        Line::from(Span::styled(
                            format!("{} {:.0}%", render_bar(needs_pct, bar_w), needs_pct),
                            Style::default().fg(Color::Cyan),
                        )),
                        blank(),
                        Line::from(Span::styled(
                            "Wants",
                            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                        )),
                        Line::from(format!("{:.2} PLN", wants_total)),
                        Line::from(Span::styled(
                            format!("{} {:.0}%", render_bar(wants_pct, bar_w), wants_pct),
                            Style::default().fg(Color::Yellow),
                        )),
                        blank(),
                        divider(),
                        blank(),
                        label("Payment Progress"),
                        Line::from(Span::styled(
                            format!("{} {:.0}%", render_bar(payment_progress, bar_w), payment_progress),
                            Style::default().fg(progress_color),
                        )),
                        blank(),
                        label(&format!("Paid    {:.2} PLN", paid)),
                        label(&format!("Unpaid  {:.2} PLN", unpaid)),
                    ]);

                    let sidebar = Paragraph::new(sidebar_text)
                        .block(Block::default().borders(Borders::ALL).title(" Overview "))
                        .alignment(Alignment::Left);
                    f.render_widget(sidebar, content[1]);

                    // Help bar
                    let help_text = Text::from(vec![Line::from(vec![
                        Span::styled(" ◄ h ", dim),
                        Span::styled("Month", Style::default().fg(Color::DarkGray)),
                        Span::styled(" l ► ", dim),
                        Span::raw("  "),
                        Span::styled("j/k ", dim),
                        Span::raw("Select  "),
                        Span::styled("i ", dim),
                        Span::raw("Add  "),
                        Span::styled("Space ", dim),
                        Span::raw("Paid  "),
                        Span::styled("t ", dim),
                        Span::raw("Category  "),
                        Span::styled("d ", dim),
                        Span::raw("Delete  "),
                        Span::styled("c ", dim),
                        Span::raw("Copy  "),
                        Span::styled("b ", dim),
                        Span::raw("Balance  "),
                        Span::styled("q ", dim),
                        Span::raw("Quit"),
                    ])]);
                    let help = Paragraph::new(help_text)
                        .block(Block::default().borders(Borders::ALL))
                        .alignment(Alignment::Center);
                    f.render_widget(help, chunks[2]);
                },

                AppMode::Overview => {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .margin(1)
                        .constraints([
                            Constraint::Length(3),
                            Constraint::Min(0),
                            Constraint::Length(3),
                        ].as_ref())
                        .split(f.size());

                    let month_names = ["January", "February", "March", "April", "May", "June",
                                     "July", "August", "September", "October", "November", "December"];
                    let month_name = month_names[app.current_month as usize - 1];
                    let title = Paragraph::new(format!("◄  {} {}  ►", month_name, app.current_year))
                        .block(Block::default().borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Cyan)))
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
                    f.render_widget(title, chunks[0]);

                    let total = app.expense_manager.get_month_total(app.current_year, app.current_month);
                    let needs_total = app.expense_manager.get_needs_total(app.current_year, app.current_month);
                    let wants_total = app.expense_manager.get_wants_total(app.current_year, app.current_month);
                    let unpaid = app.expense_manager.get_unpaid_total(app.current_year, app.current_month);
                    let paid = total - unpaid;
                    let balance = app.expense_manager.get_balance(app.current_year, app.current_month);
                    let free_money = balance - unpaid;
                    let payment_progress = if total > 0.0 { (paid / total) * 100.0 } else { 100.0 };
                    let needs_pct = if total > 0.0 { (needs_total / total) * 100.0 } else { 0.0 };
                    let wants_pct = if total > 0.0 { (wants_total / total) * 100.0 } else { 0.0 };

                    let bar_w = 20usize;
                    let free_color = if free_money >= 0.0 { Color::Green } else { Color::Red };
                    let progress_color = if payment_progress >= 80.0 { Color::Green }
                        else if payment_progress >= 50.0 { Color::Yellow }
                        else { Color::Red };
                    let dim = Style::default().fg(Color::DarkGray);

                    let content = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([
                            Constraint::Percentage(50),
                            Constraint::Percentage(50),
                        ].as_ref())
                        .split(chunks[1]);

                    let left_text = Text::from(vec![
                        Line::from(Span::styled("Balance", dim)),
                        Line::from(Span::styled(
                            format!("{:.2} PLN", balance),
                            Style::default().add_modifier(Modifier::BOLD),
                        )),
                        Line::from(""),
                        Line::from(Span::styled("Free Money", dim)),
                        Line::from(Span::styled(
                            format!("{:.2} PLN", free_money),
                            Style::default().fg(free_color).add_modifier(Modifier::BOLD),
                        )),
                        Line::from(""),
                        Line::from(Span::styled("─────────────────────────", dim)),
                        Line::from(""),
                        Line::from(Span::styled("Payment Progress", dim)),
                        Line::from(Span::styled(
                            format!("{} {:.0}%", render_bar(payment_progress, bar_w), payment_progress),
                            Style::default().fg(progress_color),
                        )),
                        Line::from(""),
                        Line::from(Span::styled(format!("Paid    {:.2} PLN", paid), dim)),
                        Line::from(Span::styled(format!("Unpaid  {:.2} PLN", unpaid), dim)),
                    ]);

                    let left = Paragraph::new(left_text)
                        .block(Block::default().borders(Borders::ALL).title(" Funds "))
                        .alignment(Alignment::Left);
                    f.render_widget(left, content[0]);

                    let right_text = Text::from(vec![
                        Line::from(Span::styled("Total Expenses", dim)),
                        Line::from(Span::styled(
                            format!("{:.2} PLN", total),
                            Style::default().add_modifier(Modifier::BOLD),
                        )),
                        Line::from(""),
                        Line::from(Span::styled("─────────────────────────", dim)),
                        Line::from(""),
                        Line::from(Span::styled(
                            "Needs",
                            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                        )),
                        Line::from(format!("{:.2} PLN", needs_total)),
                        Line::from(Span::styled(
                            format!("{} {:.0}%", render_bar(needs_pct, bar_w), needs_pct),
                            Style::default().fg(Color::Cyan),
                        )),
                        Line::from(""),
                        Line::from(Span::styled(
                            "Wants",
                            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                        )),
                        Line::from(format!("{:.2} PLN", wants_total)),
                        Line::from(Span::styled(
                            format!("{} {:.0}%", render_bar(wants_pct, bar_w), wants_pct),
                            Style::default().fg(Color::Yellow),
                        )),
                    ]);

                    let right = Paragraph::new(right_text)
                        .block(Block::default().borders(Borders::ALL).title(" Breakdown "))
                        .alignment(Alignment::Left);
                    f.render_widget(right, content[1]);

                    let help = Paragraph::new(" h/l Month  │  b Balance  │  H History  │  Esc List  │  q Quit")
                        .block(Block::default().borders(Borders::ALL))
                        .style(Style::default().fg(Color::DarkGray))
                        .alignment(Alignment::Center);
                    f.render_widget(help, chunks[2]);
                }

                AppMode::History => {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .margin(1)
                        .constraints([
                            Constraint::Length(3),
                            Constraint::Min(0),
                            Constraint::Length(3),
                        ].as_ref())
                        .split(f.size());

                    let title = Paragraph::new("Balance History")
                        .block(Block::default().borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Cyan)))
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
                    f.render_widget(title, chunks[0]);

                    let dim = Style::default().fg(Color::DarkGray);
                    let history = app.expense_manager.get_balance_history(app.current_year, app.current_month);
                    let items: Vec<Row> = history.iter().map(|log| {
                        Row::new(vec![
                            log.timestamp.format("%Y-%m-%d %H:%M").to_string(),
                            format!("{:.2}", log.balance),
                            format!("{:.2}", log.total_expenses),
                            format!("{:.2}", log.unpaid_expenses),
                            format!("{:.2}", log.remaining),
                        ])
                    }).collect();

                    let table = Table::new(items)
                        .header(Row::new(vec!["Timestamp", "Balance", "Total", "Unpaid", "Remaining"])
                            .style(dim))
                        .block(Block::default().borders(Borders::ALL))
                        .widths(&[
                            Constraint::Percentage(28),
                            Constraint::Percentage(18),
                            Constraint::Percentage(18),
                            Constraint::Percentage(18),
                            Constraint::Percentage(18),
                        ]);
                    f.render_widget(table, chunks[1]);

                    let help = Paragraph::new(" Esc Back  │  q Quit")
                        .block(Block::default().borders(Borders::ALL))
                        .style(Style::default().fg(Color::DarkGray))
                        .alignment(Alignment::Center);
                    f.render_widget(help, chunks[2]);
                }

                AppMode::AddBalance => {
                    let popup = centered_rect(40, 22, f.size());
                    let input = Paragraph::new(format!(
                        "Enter new balance:\n\n  PLN  {}\n\n  Enter to save · Esc to cancel",
                        app.input_balance
                    ))
                    .block(Block::default().borders(Borders::ALL)
                        .title(" Set Balance ")
                        .border_style(Style::default().fg(Color::Cyan)))
                    .alignment(Alignment::Center);
                    f.render_widget(Clear, popup);
                    f.render_widget(input, popup);
                }

                AppMode::ConfirmDelete => {
                    let popup = centered_rect(50, 22, f.size());
                    let text = format!(
                        "Delete this expense?\n\n  {}\n\n  y  Yes    n  No",
                        app.pending_delete_description
                    );
                    let dialog = Paragraph::new(text)
                        .block(Block::default().borders(Borders::ALL)
                            .title(" Confirm Delete ")
                            .border_style(Style::default().fg(Color::Red)))
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(Color::Red));
                    f.render_widget(Clear, popup);
                    f.render_widget(dialog, popup);
                }
            }

            // Add expense popup (overlays any mode)
            if let InputMode::Adding(field) = app.input_mode {
                let popup = centered_rect(56, 52, f.size());
                f.render_widget(Clear, popup);

                let outer = Block::default()
                    .borders(Borders::ALL)
                    .title(" New Expense ")
                    .border_style(Style::default().fg(Color::Cyan));
                let inner = outer.inner(popup);
                f.render_widget(outer, popup);

                let field_areas = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints([
                        Constraint::Length(3), // Description
                        Constraint::Length(3), // Amount
                        Constraint::Length(3), // Category
                        Constraint::Min(0),    // Help
                    ].as_ref())
                    .split(inner);

                let active = Style::default().fg(Color::Cyan);
                let inactive = Style::default().fg(Color::DarkGray);

                let desc_block = Block::default().borders(Borders::ALL).title(" Description ")
                    .border_style(if matches!(field, InputField::Description) { active } else { inactive });
                let amount_block = Block::default().borders(Borders::ALL).title(" Amount (PLN) ")
                    .border_style(if matches!(field, InputField::Amount) { active } else { inactive });
                let cat_block = Block::default().borders(Borders::ALL).title(" Category ")
                    .border_style(if matches!(field, InputField::Category) { active } else { inactive });

                f.render_widget(
                    Paragraph::new(app.input_description.as_str()).block(desc_block),
                    field_areas[0],
                );
                f.render_widget(
                    Paragraph::new(app.input_amount.as_str()).block(amount_block),
                    field_areas[1],
                );

                let need_style = if app.input_category == Category::Need {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let want_style = if app.input_category == Category::Want {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let cat_line = Line::from(vec![
                    Span::styled(
                        if app.input_category == Category::Need { "  [ ▶ Need  ] " } else { "  [   Need  ] " },
                        need_style,
                    ),
                    Span::raw("   "),
                    Span::styled(
                        if app.input_category == Category::Want { "[ ▶ Want  ]  " } else { "[   Want  ]  " },
                        want_style,
                    ),
                ]);
                f.render_widget(
                    Paragraph::new(Text::from(vec![cat_line])).block(cat_block),
                    field_areas[2],
                );

                let help = Paragraph::new(
                    Line::from(vec![
                        Span::styled("Tab ", Style::default().fg(Color::DarkGray)),
                        Span::raw("Next  "),
                        Span::styled("n/w ", Style::default().fg(Color::DarkGray)),
                        Span::raw("Category  "),
                        Span::styled("Enter ", Style::default().fg(Color::DarkGray)),
                        Span::raw("Save  "),
                        Span::styled("Esc ", Style::default().fg(Color::DarkGray)),
                        Span::raw("Cancel"),
                    ])
                )
                .alignment(Alignment::Center);
                f.render_widget(help, field_areas[3]);
            }
        })?;

        if let Event::Key(key) = event::read()? {
            match app.input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('i') => {
                        if matches!(app.app_mode, AppMode::List) {
                            app.input_mode = InputMode::Adding(InputField::Description);
                            app.input_description.clear();
                            app.input_amount.clear();
                            app.input_category = Category::Need;
                        }
                    }
                    KeyCode::Char('o') => {
                        app.app_mode = AppMode::Overview;
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        if matches!(app.app_mode, AppMode::List) {
                            if let Some(expenses) = app.expense_manager
                                .get_month_expenses(app.current_year, app.current_month)
                            {
                                let len = expenses.len();
                                if len > 0 {
                                    app.selected_expense = Some(app.selected_expense.map_or(0, |i| {
                                        if i >= len - 1 { len - 1 } else { i + 1 }
                                    }));
                                }
                            }
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if matches!(app.app_mode, AppMode::List) {
                            if app.selected_expense.is_some() {
                                app.selected_expense = Some(app.selected_expense.map_or(0, |i| {
                                    if i == 0 { 0 } else { i - 1 }
                                }));
                            } else if let Some(expenses) = app.expense_manager
                                .get_month_expenses(app.current_year, app.current_month)
                            {
                                if !expenses.is_empty() {
                                    app.selected_expense = Some(0);
                                }
                            }
                        }
                    }
                    KeyCode::Char('h') | KeyCode::Left => {
                        if matches!(app.app_mode, AppMode::List | AppMode::Overview) {
                            if app.current_month > 1 {
                                app.current_month -= 1;
                            } else {
                                app.current_month = 12;
                                app.current_year -= 1;
                            }
                            app.selected_expense = None;
                        }
                    }
                    KeyCode::Char('l') | KeyCode::Right => {
                        if matches!(app.app_mode, AppMode::List | AppMode::Overview) {
                            if app.current_month < 12 {
                                app.current_month += 1;
                            } else {
                                app.current_month = 1;
                                app.current_year += 1;
                            }
                            app.selected_expense = None;
                        }
                    }
                    KeyCode::Char(' ') => {
                        if matches!(app.app_mode, AppMode::List) {
                            if let Some(sel) = app.selected_expense {
                                let original_idx = if let Some(expenses) = app.expense_manager
                                    .get_month_expenses(app.current_year, app.current_month)
                                {
                                    sorted_expenses_grouped(expenses).get(sel).map(|(idx, _)| *idx)
                                } else {
                                    None
                                };
                                if let Some(idx) = original_idx {
                                    if let Some(expenses_mut) = app.expense_manager
                                        .get_month_expenses_mut(app.current_year, app.current_month)
                                    {
                                        if let Some(expense) = expenses_mut.get_mut(idx) {
                                            expense.toggle_paid();
                                            let _ = app.expense_manager.save();
                                        }
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Char('d') => {
                        if matches!(app.app_mode, AppMode::List) {
                            if let Some(sel) = app.selected_expense {
                                let expense_info = if let Some(expenses) = app.expense_manager
                                    .get_month_expenses(app.current_year, app.current_month)
                                {
                                    sorted_expenses_grouped(expenses)
                                        .get(sel)
                                        .map(|(idx, exp)| (*idx, exp.description.clone()))
                                } else {
                                    None
                                };
                                if let Some((idx, desc)) = expense_info {
                                    app.pending_delete_index = Some(idx);
                                    app.pending_delete_description = desc;
                                    app.app_mode = AppMode::ConfirmDelete;
                                }
                            }
                        }
                    }
                    KeyCode::Char('c') => {
                        if matches!(app.app_mode, AppMode::List) {
                            if let Some(sel) = app.selected_expense {
                                let original_idx = if let Some(expenses) = app.expense_manager
                                    .get_month_expenses(app.current_year, app.current_month)
                                {
                                    sorted_expenses_grouped(expenses).get(sel).map(|(idx, _)| *idx)
                                } else {
                                    None
                                };
                                if let Some(idx) = original_idx {
                                    app.expense_manager.copy_expense_to_next_month(
                                        app.current_year, app.current_month, idx,
                                    );
                                }
                            }
                        }
                    }
                    KeyCode::Char('t') => {
                        if matches!(app.app_mode, AppMode::List) {
                            if let Some(sel) = app.selected_expense {
                                let original_idx = if let Some(expenses) = app.expense_manager
                                    .get_month_expenses(app.current_year, app.current_month)
                                {
                                    sorted_expenses_grouped(expenses).get(sel).map(|(idx, _)| *idx)
                                } else {
                                    None
                                };
                                if let Some(idx) = original_idx {
                                    if let Some(expenses_mut) = app.expense_manager
                                        .get_month_expenses_mut(app.current_year, app.current_month)
                                    {
                                        if let Some(expense) = expenses_mut.get_mut(idx) {
                                            expense.category = expense.category.toggle();
                                            let _ = app.expense_manager.save();
                                        }
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Char('H') => {
                        if matches!(app.app_mode, AppMode::Overview) {
                            app.app_mode = AppMode::History;
                        }
                    }
                    KeyCode::Char('b') => {
                        if matches!(app.app_mode, AppMode::Overview | AppMode::List) {
                            app.app_mode = AppMode::AddBalance;
                            app.input_balance.clear();
                        }
                    }
                    KeyCode::Esc => {
                        match app.app_mode {
                            AppMode::Overview => app.app_mode = AppMode::List,
                            AppMode::History => app.app_mode = AppMode::Overview,
                            AppMode::AddBalance => app.app_mode = AppMode::List,
                            AppMode::ConfirmDelete => {
                                app.app_mode = AppMode::List;
                                app.pending_delete_index = None;
                                app.pending_delete_description.clear();
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Enter => {
                        if matches!(app.app_mode, AppMode::AddBalance) {
                            if let Ok(balance) = app.input_balance.parse::<f64>() {
                                app.expense_manager.set_balance(balance, app.current_year, app.current_month);
                                app.app_mode = AppMode::List;
                            }
                        }
                    }
                    KeyCode::Char(c) => {
                        if matches!(app.app_mode, AppMode::AddBalance) {
                            if c.is_digit(10) || c == '.' {
                                app.input_balance.push(c);
                            }
                        } else if matches!(app.app_mode, AppMode::ConfirmDelete) {
                            match c {
                                'y' | 'Y' => {
                                    if let Some(idx) = app.pending_delete_index {
                                        let sel = app.selected_expense;
                                        if app.expense_manager.delete_expense(app.current_year, app.current_month, idx) {
                                            let new_len = app.expense_manager
                                                .get_month_expenses(app.current_year, app.current_month)
                                                .map(|e| e.len())
                                                .unwrap_or(0);
                                            if new_len == 0 {
                                                app.selected_expense = None;
                                            } else if let Some(s) = sel {
                                                if s >= new_len { app.selected_expense = Some(new_len - 1); }
                                            }
                                        }
                                    }
                                    app.app_mode = AppMode::List;
                                    app.pending_delete_index = None;
                                    app.pending_delete_description.clear();
                                }
                                'n' | 'N' => {
                                    app.app_mode = AppMode::List;
                                    app.pending_delete_index = None;
                                    app.pending_delete_description.clear();
                                }
                                _ => {}
                            }
                        }
                    }
                    KeyCode::Backspace => {
                        if matches!(app.app_mode, AppMode::AddBalance) {
                            app.input_balance.pop();
                        }
                    }
                    _ => {}
                },
                InputMode::Adding(field) => match key.code {
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                    }
                    KeyCode::Tab => {
                        app.input_mode = InputMode::Adding(match field {
                            InputField::Description => InputField::Amount,
                            InputField::Amount => InputField::Category,
                            InputField::Category => InputField::Description,
                        });
                    }
                    KeyCode::Enter => {
                        if !app.input_description.is_empty() && !app.input_amount.is_empty() {
                            if let Ok(amount) = app.input_amount.parse::<f64>() {
                                let expense = Expense::new(
                                    app.input_description.clone(),
                                    amount,
                                    app.input_category,
                                );
                                app.expense_manager.add_expense(app.current_year, app.current_month, expense);
                                let _ = app.expense_manager.save();
                                app.input_mode = InputMode::Normal;
                                app.input_description.clear();
                                app.input_amount.clear();
                                app.input_category = Category::Need;
                            }
                        }
                    }
                    KeyCode::Left | KeyCode::Right => {
                        if matches!(field, InputField::Category) {
                            app.input_category = app.input_category.toggle();
                        }
                    }
                    KeyCode::Char(c) => {
                        match field {
                            InputField::Description => { app.input_description.push(c); }
                            InputField::Amount => {
                                if c.is_digit(10) || c == '.' { app.input_amount.push(c); }
                            }
                            InputField::Category => match c {
                                ' ' => { app.input_category = app.input_category.toggle(); }
                                'n' | 'N' => { app.input_category = Category::Need; }
                                'w' | 'W' => { app.input_category = Category::Want; }
                                _ => {}
                            },
                        }
                    }
                    KeyCode::Backspace => {
                        match field {
                            InputField::Description => { app.input_description.pop(); }
                            InputField::Amount => { app.input_amount.pop(); }
                            InputField::Category => {}
                        }
                    }
                    _ => {}
                },
            }
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ].as_ref())
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ].as_ref())
        .split(popup_layout[1])[1]
}

fn main() -> Result<(), io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = App::new();
    let res = run_app(&mut terminal, app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}
