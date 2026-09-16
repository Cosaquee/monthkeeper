use chrono::{DateTime, Datelike, Local};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Row, Table},
    Terminal,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

const DATA_FILE: &str = "expenses_data.json";
const LOG_FILE: &str = "expenses.log";

fn log_message(message: &str) {
    if let Some(mut data_dir) = dirs::data_dir() {
        data_dir.push("recurring_costs");
        let _ = fs::create_dir_all(&data_dir);
        data_dir.push(LOG_FILE);

        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&data_dir) {
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
                expenses
                    .iter()
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
                expenses
                    .iter()
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
                expenses
                    .iter()
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
            log_message(&format!(
                "Copied expense '{}' to {}-{:02}",
                expense.description, next_year, next_month
            ));
            return true;
        }
        false
    }
}

fn sorted_expenses_grouped(expenses: &[Expense]) -> Vec<(usize, &Expense)> {
    let mut needs: Vec<(usize, &Expense)> = expenses
        .iter()
        .enumerate()
        .filter(|(_, e)| e.category == Category::Need)
        .collect();
    needs.sort_by(|a, b| {
        a.1.description
            .to_lowercase()
            .cmp(&b.1.description.to_lowercase())
    });

    let mut wants: Vec<(usize, &Expense)> = expenses
        .iter()
        .enumerate()
        .filter(|(_, e)| e.category == Category::Want)
        .collect();
    wants.sort_by(|a, b| {
        a.1.description
            .to_lowercase()
            .cmp(&b.1.description.to_lowercase())
    });

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

impl AppMode {
    fn is_content_view(self) -> bool {
        matches!(self, AppMode::List | AppMode::Overview)
    }
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
    input_error: Option<String>,
    balance_return_mode: AppMode,
    pending_delete_index: Option<usize>,
    pending_delete_description: String,
    pending_delete_amount: f64,
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
            input_error: None,
            balance_return_mode: AppMode::List,
            pending_delete_index: None,
            pending_delete_description: String::new(),
            pending_delete_amount: 0.0,
        }
    }
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| {
            match app.app_mode {
                AppMode::List => {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .margin(1)
                        .constraints(
                            [
                                Constraint::Length(3),
                                Constraint::Min(0),
                                Constraint::Length(3),
                            ]
                            .as_ref(),
                        )
                        .split(f.size());

                    // Title: centred month + nav hints
                    let month_names = [
                        "January",
                        "February",
                        "March",
                        "April",
                        "May",
                        "June",
                        "July",
                        "August",
                        "September",
                        "October",
                        "November",
                        "December",
                    ];
                    let month_name = month_names[app.current_month as usize - 1];
                    let title_text = format!("◄  {} {}  ►", month_name, app.current_year);
                    let title = Paragraph::new(title_text)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(Style::default().fg(Color::Cyan)),
                        )
                        .alignment(Alignment::Center)
                        .style(
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        );
                    f.render_widget(title, chunks[0]);

                    // Main area: expense list + sidebar
                    let content = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints(
                            [Constraint::Percentage(65), Constraint::Percentage(35)].as_ref(),
                        )
                        .split(chunks[1]);

                    // ── Expense table ──────────────────────────────────────────

                    let empty_vec = Vec::new();
                    let expenses = app
                        .expense_manager
                        .get_month_expenses(app.current_year, app.current_month)
                        .unwrap_or(&empty_vec);
                    let sorted = sorted_expenses_grouped(expenses);
                    let n_needs = sorted
                        .iter()
                        .filter(|(_, e)| e.category == Category::Need)
                        .count();

                    let dim = Style::default().fg(Color::DarkGray);
                    let selected_style = Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD);

                    let mut rows: Vec<Row> = Vec::new();

                    // Needs section
                    rows.push(
                        Row::new(vec!["▸ NEEDS", "", ""]).style(
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                    );
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
                        let status = if expense.is_paid {
                            "✓  paid"
                        } else {
                            "✗  due "
                        };
                        rows.push(
                            Row::new(vec![
                                format!("{}{}", prefix, expense.description),
                                format!("{:.2}", expense.amount),
                                status.to_string(),
                            ])
                            .style(base),
                        );
                    }

                    // Wants section
                    rows.push(
                        Row::new(vec!["▸ WANTS", "", ""]).style(
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ),
                    );
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
                        let status = if expense.is_paid {
                            "✓  paid"
                        } else {
                            "✗  due "
                        };
                        rows.push(
                            Row::new(vec![
                                format!("{}{}", prefix, expense.description),
                                format!("{:.2}", expense.amount),
                                status.to_string(),
                            ])
                            .style(base),
                        );
                    }

                    let table = Table::new(rows)
                        .header(Row::new(vec!["  Expense", "Amount", "Status"]).style(dim))
                        .block(Block::default().borders(Borders::ALL).title(" Expenses "))
                        .widths(&[
                            Constraint::Percentage(52),
                            Constraint::Percentage(24),
                            Constraint::Percentage(24),
                        ]);
                    f.render_widget(table, content[0]);

                    // ── Sidebar ────────────────────────────────────────────────

                    let total = app
                        .expense_manager
                        .get_month_total(app.current_year, app.current_month);
                    let needs_total = app
                        .expense_manager
                        .get_needs_total(app.current_year, app.current_month);
                    let wants_total = app
                        .expense_manager
                        .get_wants_total(app.current_year, app.current_month);
                    let unpaid = app
                        .expense_manager
                        .get_unpaid_total(app.current_year, app.current_month);
                    let paid = total - unpaid;
                    let balance = app
                        .expense_manager
                        .get_balance(app.current_year, app.current_month);
                    let free_money = balance - unpaid;
                    let payment_progress = if total > 0.0 {
                        (paid / total) * 100.0
                    } else {
                        100.0
                    };
                    let needs_pct = if total > 0.0 {
                        (needs_total / total) * 100.0
                    } else {
                        0.0
                    };
                    let wants_pct = if total > 0.0 {
                        (wants_total / total) * 100.0
                    } else {
                        0.0
                    };

                    // Fill the panel at any terminal width instead of limiting the visual
                    // summaries to the old fixed 16-character bars.
                    let sidebar_width = content[1].width.saturating_sub(4) as usize;
                    let bar_w = sidebar_width.saturating_sub(5).max(4);
                    let free_color = if free_money >= 0.0 {
                        Color::Green
                    } else {
                        Color::Red
                    };
                    let progress_color = if payment_progress >= 80.0 {
                        Color::Green
                    } else if payment_progress >= 50.0 {
                        Color::Yellow
                    } else {
                        Color::Red
                    };

                    let metric = |label: &str,
                                  value: String,
                                  label_style: Style,
                                  value_style: Style|
                     -> Line {
                        let gap = sidebar_width
                            .saturating_sub(label.chars().count() + value.chars().count());
                        Line::from(vec![
                            Span::styled(label.to_string(), label_style),
                            Span::raw(" ".repeat(gap.max(1))),
                            Span::styled(value, value_style),
                        ])
                    };
                    let divider =
                        || -> Line { Line::from(Span::styled("─".repeat(sidebar_width), dim)) };
                    let blank = || -> Line { Line::from("") };
                    let section = |title: &str| -> Line {
                        Line::from(Span::styled(
                            title.to_string(),
                            Style::default()
                                .fg(Color::DarkGray)
                                .add_modifier(Modifier::BOLD),
                        ))
                    };

                    let sidebar_text = Text::from(vec![
                        section("FUNDS"),
                        blank(),
                        metric(
                            "Balance",
                            format!("{:.2} PLN", balance),
                            dim,
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        metric(
                            "Free money",
                            format!("{:.2} PLN", free_money),
                            dim,
                            Style::default().fg(free_color).add_modifier(Modifier::BOLD),
                        ),
                        blank(),
                        divider(),
                        blank(),
                        section("EXPENSE MIX"),
                        blank(),
                        metric(
                            "Needs",
                            format!("{:.2} PLN", needs_total),
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                            Style::default().fg(Color::Cyan),
                        ),
                        Line::from(Span::styled(
                            format!("{} {:.0}%", render_bar(needs_pct, bar_w), needs_pct),
                            Style::default().fg(Color::Cyan),
                        )),
                        blank(),
                        metric(
                            "Wants",
                            format!("{:.2} PLN", wants_total),
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                            Style::default().fg(Color::Yellow),
                        ),
                        Line::from(Span::styled(
                            format!("{} {:.0}%", render_bar(wants_pct, bar_w), wants_pct),
                            Style::default().fg(Color::Yellow),
                        )),
                        blank(),
                        divider(),
                        blank(),
                        section("PAYMENTS"),
                        blank(),
                        metric(
                            "Paid",
                            format!("{:.2} PLN", paid),
                            dim,
                            Style::default().fg(Color::Green),
                        ),
                        metric(
                            "Still due",
                            format!("{:.2} PLN", unpaid),
                            dim,
                            Style::default().fg(if unpaid > 0.0 {
                                Color::Red
                            } else {
                                Color::Green
                            }),
                        ),
                        blank(),
                        Line::from(Span::styled(
                            format!(
                                "{} {:.0}%",
                                render_bar(payment_progress, bar_w),
                                payment_progress
                            ),
                            Style::default().fg(progress_color),
                        )),
                        Line::from(Span::styled("monthly expenses paid", dim)),
                    ]);

                    let sidebar = Paragraph::new(sidebar_text)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" Overview ")
                                .padding(Padding::horizontal(1)),
                        )
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
                }

                AppMode::Overview => {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .margin(1)
                        .constraints(
                            [
                                Constraint::Length(3),
                                Constraint::Min(0),
                                Constraint::Length(3),
                            ]
                            .as_ref(),
                        )
                        .split(f.size());

                    let month_names = [
                        "January",
                        "February",
                        "March",
                        "April",
                        "May",
                        "June",
                        "July",
                        "August",
                        "September",
                        "October",
                        "November",
                        "December",
                    ];
                    let month_name = month_names[app.current_month as usize - 1];
                    let title =
                        Paragraph::new(format!("◄  {} {}  ►", month_name, app.current_year))
                            .block(
                                Block::default()
                                    .borders(Borders::ALL)
                                    .border_style(Style::default().fg(Color::Cyan)),
                            )
                            .alignment(Alignment::Center)
                            .style(
                                Style::default()
                                    .fg(Color::Cyan)
                                    .add_modifier(Modifier::BOLD),
                            );
                    f.render_widget(title, chunks[0]);

                    let total = app
                        .expense_manager
                        .get_month_total(app.current_year, app.current_month);
                    let needs_total = app
                        .expense_manager
                        .get_needs_total(app.current_year, app.current_month);
                    let wants_total = app
                        .expense_manager
                        .get_wants_total(app.current_year, app.current_month);
                    let unpaid = app
                        .expense_manager
                        .get_unpaid_total(app.current_year, app.current_month);
                    let paid = total - unpaid;
                    let balance = app
                        .expense_manager
                        .get_balance(app.current_year, app.current_month);
                    let free_money = balance - unpaid;
                    let payment_progress = if total > 0.0 {
                        (paid / total) * 100.0
                    } else {
                        100.0
                    };
                    let needs_pct = if total > 0.0 {
                        (needs_total / total) * 100.0
                    } else {
                        0.0
                    };
                    let wants_pct = if total > 0.0 {
                        (wants_total / total) * 100.0
                    } else {
                        0.0
                    };

                    let bar_w = 20usize;
                    let free_color = if free_money >= 0.0 {
                        Color::Green
                    } else {
                        Color::Red
                    };
                    let progress_color = if payment_progress >= 80.0 {
                        Color::Green
                    } else if payment_progress >= 50.0 {
                        Color::Yellow
                    } else {
                        Color::Red
                    };
                    let dim = Style::default().fg(Color::DarkGray);

                    let content = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints(
                            [Constraint::Percentage(50), Constraint::Percentage(50)].as_ref(),
                        )
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
                            format!(
                                "{} {:.0}%",
                                render_bar(payment_progress, bar_w),
                                payment_progress
                            ),
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
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        )),
                        Line::from(format!("{:.2} PLN", needs_total)),
                        Line::from(Span::styled(
                            format!("{} {:.0}%", render_bar(needs_pct, bar_w), needs_pct),
                            Style::default().fg(Color::Cyan),
                        )),
                        Line::from(""),
                        Line::from(Span::styled(
                            "Wants",
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
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

                    let help = Paragraph::new(
                        " h/l Month  │  b Balance  │  H History  │  Esc List  │  q Quit",
                    )
                    .block(Block::default().borders(Borders::ALL))
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center);
                    f.render_widget(help, chunks[2]);
                }

                AppMode::History => {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .margin(1)
                        .constraints(
                            [
                                Constraint::Length(3),
                                Constraint::Min(0),
                                Constraint::Length(3),
                            ]
                            .as_ref(),
                        )
                        .split(f.size());

                    let title = Paragraph::new("Balance History")
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(Style::default().fg(Color::Cyan)),
                        )
                        .alignment(Alignment::Center)
                        .style(
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        );
                    f.render_widget(title, chunks[0]);

                    let dim = Style::default().fg(Color::DarkGray);
                    let history = app
                        .expense_manager
                        .get_balance_history(app.current_year, app.current_month);
                    let items: Vec<Row> = history
                        .iter()
                        .map(|log| {
                            Row::new(vec![
                                log.timestamp.format("%Y-%m-%d %H:%M").to_string(),
                                format!("{:.2}", log.balance),
                                format!("{:.2}", log.total_expenses),
                                format!("{:.2}", log.unpaid_expenses),
                                format!("{:.2}", log.remaining),
                            ])
                        })
                        .collect();

                    let table = Table::new(items)
                        .header(
                            Row::new(vec!["Timestamp", "Balance", "Total", "Unpaid", "Remaining"])
                                .style(dim),
                        )
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
                    let popup = centered_dialog(54, 13, f.size());
                    f.render_widget(Clear, popup);
                    f.render_widget(
                        Block::default().style(Style::default().bg(Color::Black)),
                        popup,
                    );

                    let outer = Block::default()
                        .borders(Borders::ALL)
                        .title(" Update balance ")
                        .title_alignment(Alignment::Center)
                        .border_style(Style::default().fg(Color::Cyan));
                    let inner = outer.inner(popup);
                    f.render_widget(outer, popup);

                    let areas = Layout::default()
                        .direction(Direction::Vertical)
                        .margin(1)
                        .constraints([
                            Constraint::Length(2),
                            Constraint::Length(3),
                            Constraint::Length(2),
                            Constraint::Min(1),
                        ])
                        .split(inner);
                    let current = app
                        .expense_manager
                        .get_balance(app.current_year, app.current_month);
                    f.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::styled("Current balance  ", Style::default().fg(Color::DarkGray)),
                            Span::styled(
                                format!("{current:.2} PLN"),
                                Style::default().add_modifier(Modifier::BOLD),
                            ),
                        ]))
                        .alignment(Alignment::Center),
                        areas[0],
                    );
                    let value = if app.input_balance.is_empty() {
                        Line::from(vec![
                            Span::styled("Enter amount", Style::default().fg(Color::DarkGray)),
                            Span::styled("█", Style::default().fg(Color::Cyan)),
                        ])
                    } else {
                        Line::from(vec![
                            Span::raw(format!("{} ", app.input_balance)),
                            Span::styled("PLN █", Style::default().fg(Color::Cyan)),
                        ])
                    };
                    f.render_widget(
                        Paragraph::new(value)
                            .block(
                                Block::default()
                                    .borders(Borders::ALL)
                                    .border_style(Style::default().fg(Color::Cyan)),
                            )
                            .alignment(Alignment::Center),
                        areas[1],
                    );
                    if let Some(error) = &app.input_error {
                        f.render_widget(
                            Paragraph::new(error.as_str())
                                .style(Style::default().fg(Color::Red))
                                .alignment(Alignment::Center),
                            areas[2],
                        );
                    }
                    f.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::styled(
                                " Enter ",
                                Style::default()
                                    .fg(Color::Black)
                                    .bg(Color::Cyan)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(" Save balance    "),
                            Span::styled(" Esc ", Style::default().fg(Color::DarkGray)),
                            Span::raw("Cancel"),
                        ]))
                        .alignment(Alignment::Center),
                        areas[3],
                    );
                }

                AppMode::ConfirmDelete => {
                    let popup = centered_dialog(58, 12, f.size());
                    f.render_widget(Clear, popup);
                    f.render_widget(
                        Block::default().style(Style::default().bg(Color::Black)),
                        popup,
                    );

                    let outer = Block::default()
                        .borders(Borders::ALL)
                        .title(" Delete expense? ")
                        .title_alignment(Alignment::Center)
                        .border_style(Style::default().fg(Color::Red));
                    let inner = outer.inner(popup);
                    f.render_widget(outer, popup);
                    let areas = Layout::default()
                        .direction(Direction::Vertical)
                        .margin(1)
                        .constraints([
                            Constraint::Length(2),
                            Constraint::Length(2),
                            Constraint::Length(2),
                            Constraint::Min(1),
                        ])
                        .split(inner);
                    f.render_widget(
                        Paragraph::new("This will permanently remove the expense from this month.")
                            .style(Style::default().fg(Color::DarkGray))
                            .alignment(Alignment::Center),
                        areas[0],
                    );
                    f.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::styled(
                                app.pending_delete_description.clone(),
                                Style::default().add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!("  ·  {:.2} PLN", app.pending_delete_amount),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]))
                        .alignment(Alignment::Center),
                        areas[1],
                    );
                    f.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::styled(
                                " y / Enter ",
                                Style::default()
                                    .fg(Color::White)
                                    .bg(Color::Red)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(" Delete    "),
                            Span::styled(" n / Esc ", Style::default().fg(Color::DarkGray)),
                            Span::raw("Keep expense"),
                        ]))
                        .alignment(Alignment::Center),
                        areas[3],
                    );
                }
            }

            // Add expense popup (overlays any mode)
            if let InputMode::Adding(field) = app.input_mode {
                let popup = centered_dialog(64, 18, f.size());
                f.render_widget(Clear, popup);
                f.render_widget(
                    Block::default().style(Style::default().bg(Color::Black)),
                    popup,
                );

                let outer = Block::default()
                    .borders(Borders::ALL)
                    .title(" New Expense ")
                    .title_alignment(Alignment::Center)
                    .border_style(Style::default().fg(Color::Cyan));
                let inner = outer.inner(popup);
                f.render_widget(outer, popup);

                let field_areas = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints(
                        [
                            Constraint::Length(3), // Description
                            Constraint::Length(3), // Amount
                            Constraint::Length(3), // Category
                            Constraint::Length(2), // Validation
                            Constraint::Min(1),    // Help
                        ]
                        .as_ref(),
                    )
                    .split(inner);

                let active = Style::default().fg(Color::Cyan);
                let inactive = Style::default().fg(Color::DarkGray);

                let desc_block = Block::default()
                    .borders(Borders::ALL)
                    .title(" Description ")
                    .border_style(if matches!(field, InputField::Description) {
                        active
                    } else {
                        inactive
                    });
                let amount_block = Block::default()
                    .borders(Borders::ALL)
                    .title(" Amount (PLN) ")
                    .border_style(if matches!(field, InputField::Amount) {
                        active
                    } else {
                        inactive
                    });
                let cat_block = Block::default()
                    .borders(Borders::ALL)
                    .title(" Category ")
                    .border_style(if matches!(field, InputField::Category) {
                        active
                    } else {
                        inactive
                    });

                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::raw(app.input_description.as_str()),
                        Span::styled(
                            if matches!(field, InputField::Description) {
                                "█"
                            } else {
                                ""
                            },
                            active,
                        ),
                    ]))
                    .block(desc_block),
                    field_areas[0],
                );
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::raw(app.input_amount.as_str()),
                        Span::styled(
                            if matches!(field, InputField::Amount) {
                                "█"
                            } else {
                                ""
                            },
                            active,
                        ),
                    ]))
                    .block(amount_block),
                    field_areas[1],
                );

                let need_style = if app.input_category == Category::Need {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let want_style = if app.input_category == Category::Want {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let cat_line = Line::from(vec![
                    Span::styled(
                        if app.input_category == Category::Need {
                            "  [ ▶ Need  ] "
                        } else {
                            "  [   Need  ] "
                        },
                        need_style,
                    ),
                    Span::raw("   "),
                    Span::styled(
                        if app.input_category == Category::Want {
                            "[ ▶ Want  ]  "
                        } else {
                            "[   Want  ]  "
                        },
                        want_style,
                    ),
                ]);
                f.render_widget(
                    Paragraph::new(Text::from(vec![cat_line])).block(cat_block),
                    field_areas[2],
                );

                if let Some(error) = &app.input_error {
                    f.render_widget(
                        Paragraph::new(error.as_str())
                            .style(Style::default().fg(Color::Red))
                            .alignment(Alignment::Center),
                        field_areas[3],
                    );
                }

                let help = Paragraph::new(Line::from(vec![
                    Span::styled("Tab ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Next  "),
                    Span::styled("n/w ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Category  "),
                    Span::styled("Enter ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Save  "),
                    Span::styled("Esc ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Cancel"),
                ]))
                .alignment(Alignment::Center);
                f.render_widget(help, field_areas[4]);
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
                            app.input_error = None;
                        }
                    }
                    KeyCode::Char('o') => {
                        app.app_mode = AppMode::Overview;
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        if matches!(app.app_mode, AppMode::List) {
                            if let Some(expenses) = app
                                .expense_manager
                                .get_month_expenses(app.current_year, app.current_month)
                            {
                                let len = expenses.len();
                                if len > 0 {
                                    app.selected_expense =
                                        Some(app.selected_expense.map_or(0, |i| {
                                            if i >= len - 1 {
                                                len - 1
                                            } else {
                                                i + 1
                                            }
                                        }));
                                }
                            }
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if matches!(app.app_mode, AppMode::List) {
                            if app.selected_expense.is_some() {
                                app.selected_expense = Some(app.selected_expense.map_or(0, |i| {
                                    if i == 0 {
                                        0
                                    } else {
                                        i - 1
                                    }
                                }));
                            } else if let Some(expenses) = app
                                .expense_manager
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
                                let original_idx = if let Some(expenses) = app
                                    .expense_manager
                                    .get_month_expenses(app.current_year, app.current_month)
                                {
                                    sorted_expenses_grouped(expenses)
                                        .get(sel)
                                        .map(|(idx, _)| *idx)
                                } else {
                                    None
                                };
                                if let Some(idx) = original_idx {
                                    if let Some(expenses_mut) = app
                                        .expense_manager
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
                                let expense_info = if let Some(expenses) = app
                                    .expense_manager
                                    .get_month_expenses(app.current_year, app.current_month)
                                {
                                    sorted_expenses_grouped(expenses)
                                        .get(sel)
                                        .map(|(idx, exp)| {
                                            (*idx, exp.description.clone(), exp.amount)
                                        })
                                } else {
                                    None
                                };
                                if let Some((idx, desc, amount)) = expense_info {
                                    app.pending_delete_index = Some(idx);
                                    app.pending_delete_description = desc;
                                    app.pending_delete_amount = amount;
                                    app.app_mode = AppMode::ConfirmDelete;
                                }
                            }
                        }
                    }
                    KeyCode::Char('c') => {
                        if matches!(app.app_mode, AppMode::List) {
                            if let Some(sel) = app.selected_expense {
                                let original_idx = if let Some(expenses) = app
                                    .expense_manager
                                    .get_month_expenses(app.current_year, app.current_month)
                                {
                                    sorted_expenses_grouped(expenses)
                                        .get(sel)
                                        .map(|(idx, _)| *idx)
                                } else {
                                    None
                                };
                                if let Some(idx) = original_idx {
                                    app.expense_manager.copy_expense_to_next_month(
                                        app.current_year,
                                        app.current_month,
                                        idx,
                                    );
                                }
                            }
                        }
                    }
                    KeyCode::Char('t') => {
                        if matches!(app.app_mode, AppMode::List) {
                            if let Some(sel) = app.selected_expense {
                                let original_idx = if let Some(expenses) = app
                                    .expense_manager
                                    .get_month_expenses(app.current_year, app.current_month)
                                {
                                    sorted_expenses_grouped(expenses)
                                        .get(sel)
                                        .map(|(idx, _)| *idx)
                                } else {
                                    None
                                };
                                if let Some(idx) = original_idx {
                                    if let Some(expenses_mut) = app
                                        .expense_manager
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
                        if app.app_mode.is_content_view() {
                            app.balance_return_mode = app.app_mode;
                            app.app_mode = AppMode::AddBalance;
                            app.input_balance.clear();
                            app.input_error = None;
                        }
                    }
                    KeyCode::Esc => match app.app_mode {
                        AppMode::Overview => app.app_mode = AppMode::List,
                        AppMode::History => app.app_mode = AppMode::Overview,
                        AppMode::AddBalance => {
                            app.app_mode = app.balance_return_mode;
                            app.input_error = None;
                        }
                        AppMode::ConfirmDelete => {
                            app.app_mode = AppMode::List;
                            app.pending_delete_index = None;
                            app.pending_delete_description.clear();
                            app.pending_delete_amount = 0.0;
                        }
                        _ => {}
                    },
                    KeyCode::Enter => {
                        if matches!(app.app_mode, AppMode::AddBalance) {
                            if let Some(balance) = parse_non_negative_amount(&app.input_balance) {
                                app.expense_manager.set_balance(
                                    balance,
                                    app.current_year,
                                    app.current_month,
                                );
                                app.app_mode = app.balance_return_mode;
                                app.input_error = None;
                            } else {
                                app.input_error =
                                    Some("Enter a valid amount of 0 or more.".to_string());
                            }
                        } else if matches!(app.app_mode, AppMode::ConfirmDelete) {
                            confirm_delete(&mut app);
                        }
                    }
                    KeyCode::Char(c) => {
                        if matches!(app.app_mode, AppMode::AddBalance) {
                            if c.is_ascii_digit() || c == '.' || c == ',' {
                                app.input_balance.push(c);
                                app.input_error = None;
                            }
                        } else if matches!(app.app_mode, AppMode::ConfirmDelete) {
                            match c {
                                'y' | 'Y' => {
                                    confirm_delete(&mut app);
                                }
                                'n' | 'N' => {
                                    app.app_mode = AppMode::List;
                                    app.pending_delete_index = None;
                                    app.pending_delete_description.clear();
                                    app.pending_delete_amount = 0.0;
                                }
                                _ => {}
                            }
                        }
                    }
                    KeyCode::Backspace => {
                        if matches!(app.app_mode, AppMode::AddBalance) {
                            app.input_balance.pop();
                            app.input_error = None;
                        }
                    }
                    _ => {}
                },
                InputMode::Adding(field) => match key.code {
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                        app.input_error = None;
                    }
                    KeyCode::Tab => {
                        app.input_error = None;
                        app.input_mode = InputMode::Adding(match field {
                            InputField::Description => InputField::Amount,
                            InputField::Amount => InputField::Category,
                            InputField::Category => InputField::Description,
                        });
                    }
                    KeyCode::Enter => {
                        let description = app.input_description.trim();
                        if description.is_empty() {
                            app.input_error = Some("Add a description before saving.".to_string());
                            app.input_mode = InputMode::Adding(InputField::Description);
                        } else if let Some(amount) = parse_positive_amount(&app.input_amount) {
                            let expense =
                                Expense::new(description.to_string(), amount, app.input_category);
                            app.expense_manager.add_expense(
                                app.current_year,
                                app.current_month,
                                expense,
                            );
                            let _ = app.expense_manager.save();
                            app.input_mode = InputMode::Normal;
                            app.input_description.clear();
                            app.input_amount.clear();
                            app.input_category = Category::Need;
                            app.input_error = None;
                        } else {
                            app.input_error = Some("Enter an amount greater than 0.".to_string());
                            app.input_mode = InputMode::Adding(InputField::Amount);
                        }
                    }
                    KeyCode::Left | KeyCode::Right => {
                        if matches!(field, InputField::Category) {
                            app.input_category = app.input_category.toggle();
                        }
                    }
                    KeyCode::Char(c) => match field {
                        InputField::Description => {
                            app.input_description.push(c);
                            app.input_error = None;
                        }
                        InputField::Amount => {
                            if c.is_ascii_digit() || c == '.' || c == ',' {
                                app.input_amount.push(c);
                                app.input_error = None;
                            }
                        }
                        InputField::Category => match c {
                            ' ' => {
                                app.input_category = app.input_category.toggle();
                            }
                            'n' | 'N' => {
                                app.input_category = Category::Need;
                            }
                            'w' | 'W' => {
                                app.input_category = Category::Want;
                            }
                            _ => {}
                        },
                    },
                    KeyCode::Backspace => {
                        match field {
                            InputField::Description => {
                                app.input_description.pop();
                            }
                            InputField::Amount => {
                                app.input_amount.pop();
                            }
                            InputField::Category => {}
                        }
                        app.input_error = None;
                    }
                    _ => {}
                },
            }
        }
    }
}

fn parse_non_negative_amount(value: &str) -> Option<f64> {
    let amount = value.trim().replace(',', ".").parse::<f64>().ok()?;
    (amount.is_finite() && amount >= 0.0).then_some(amount)
}

fn parse_positive_amount(value: &str) -> Option<f64> {
    parse_non_negative_amount(value).filter(|amount| *amount > 0.0)
}

fn confirm_delete(app: &mut App) {
    if let Some(idx) = app.pending_delete_index {
        let previous_selection = app.selected_expense;
        if app
            .expense_manager
            .delete_expense(app.current_year, app.current_month, idx)
        {
            let new_len = app
                .expense_manager
                .get_month_expenses(app.current_year, app.current_month)
                .map(Vec::len)
                .unwrap_or(0);
            app.selected_expense = match (new_len, previous_selection) {
                (0, _) => None,
                (len, Some(selected)) if selected >= len => Some(len - 1),
                (_, selection) => selection,
            };
        }
    }
    app.app_mode = AppMode::List;
    app.pending_delete_index = None;
    app.pending_delete_description.clear();
    app.pending_delete_amount = 0.0;
}

fn centered_dialog(max_width: u16, height: u16, area: Rect) -> Rect {
    let width = max_width.min(area.width.saturating_sub(2)).max(1);
    let height = height.min(area.height.saturating_sub(2)).max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::{parse_non_negative_amount, parse_positive_amount};

    #[test]
    fn parses_amounts_with_dot_or_comma() {
        assert_eq!(parse_positive_amount("12.50"), Some(12.5));
        assert_eq!(parse_positive_amount("12,50"), Some(12.5));
    }

    #[test]
    fn rejects_invalid_amounts() {
        assert_eq!(parse_positive_amount("0"), None);
        assert_eq!(parse_positive_amount("-5"), None);
        assert_eq!(parse_positive_amount("not a number"), None);
        assert_eq!(parse_non_negative_amount("NaN"), None);
    }
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
