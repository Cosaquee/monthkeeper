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
    style::{Color, Style},
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expense {
    description: String,
    amount: f64,
    is_paid: bool,
}

impl Expense {
    pub fn new(description: String, amount: f64) -> Self {
        Self {
            description,
            amount,
            is_paid: false,
        }
    }

    pub fn toggle_paid(&mut self) {
        self.is_paid = !self.is_paid;
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExpenseManager {
    expenses: HashMap<String, Vec<Expense>>,
    balance: f64,
    balance_history: Vec<BalanceLog>,
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
                    balance: 0.0,
                    balance_history: Vec::new(),
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
        self.balance = balance;
        let total = self.get_month_total(year, month);
        let unpaid = self.get_unpaid_total(year, month);
        let remaining = balance - unpaid;

        self.balance_history.push(BalanceLog {
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

    pub fn get_balance(&self) -> f64 {
        self.balance
    }

    pub fn get_balance_history(&self) -> &Vec<BalanceLog> {
        &self.balance_history
    }

    pub fn get_month_expenses(&self, year: i32, month: u32) -> Option<&Vec<Expense>> {
        let key = Self::get_key(year, month);
        self.expenses.get(&key)
    }

    pub fn get_month_expenses_mut(&mut self, year: i32, month: u32) -> Option<&mut Vec<Expense>> {
        let key = Self::get_key(year, month);
        self.expenses.get_mut(&key)
    }

    pub fn get_month_total(&self, year: i32, month: u32) -> f64 {
        let key = Self::get_key(year, month);
        self.expenses
            .get(&key)
            .map(|expenses| {
                expenses.iter().map(|e| e.amount).sum()
            })
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
}

#[derive(Copy, Clone)]
enum AppMode {
    List,
    Overview,
    History,
    AddBalance,
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

                    let month_names = ["January", "February", "March", "April", "May", "June",
                                     "July", "August", "September", "October", "November", "December"];
                    let month_name = month_names[app.current_month as usize - 1];
                    let month_year = format!("{} {}", month_name, app.current_year);
                    let title = Paragraph::new(month_year)
                        .block(Block::default().borders(Borders::ALL).title("Current Period"));
                    f.render_widget(title, chunks[0]);

                    let empty_vec = Vec::new();
                    let expenses = app.expense_manager
                        .get_month_expenses(app.current_year, app.current_month)
                        .unwrap_or(&empty_vec);

                    let items: Vec<Row> = expenses
                        .iter()
                        .enumerate()
                        .map(|(i, expense)| {
                            let style = if Some(i) == app.selected_expense {
                                Style::default().fg(Color::Yellow)
                            } else {
                                Style::default()
                            };
                            let status_style = if expense.is_paid {
                                Style::default().fg(Color::Green)
                            } else {
                                Style::default().fg(Color::Red)
                            };
                            Row::new(vec![
                                expense.description.clone(),
                                format!("{:.2} PLN", expense.amount),
                                if expense.is_paid { "✓ PAID" } else { "✗ PENDING" }.to_string(),
                            ]).style(style)
                        })
                        .collect();

                    let table = Table::new(items)
                        .header(Row::new(vec!["Description", "Amount", "Status"]).style(Style::default().fg(Color::Cyan)))
                        .block(Block::default().borders(Borders::ALL).title("Monthly Expenses"))
                        .widths(&[
                            Constraint::Percentage(50),
                            Constraint::Percentage(25),
                            Constraint::Percentage(25),
                        ])
                        .highlight_style(Style::default().fg(Color::Yellow));

                    f.render_widget(table, chunks[1]);

                    let help_text = "[h/l] Change Month | [j/k] Select | [i] Add | [Space] Toggle Paid | [d] Delete | [o] Overview | [q] Quit";
                    let status = Paragraph::new(help_text)
                        .block(Block::default().borders(Borders::ALL));
                    f.render_widget(status, chunks[2]);
                },
                AppMode::Overview => {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .margin(1)
                        .constraints([
                            Constraint::Length(3),  // Title
                            Constraint::Length(3),  // Spacer
                            Constraint::Length(12), // Financial Summary
                            Constraint::Length(3),  // Progress Bar
                            Constraint::Min(0),     // Flexible space
                            Constraint::Length(3),  // Help text
                        ].as_ref())
                        .split(f.size());

                    let month_names = ["January", "February", "March", "April", "May", "June",
                                     "July", "August", "September", "October", "November", "December"];
                    let month_name = month_names[app.current_month as usize - 1];
                    let month_year = format!("{} {}", month_name, app.current_year);
                    let title = Paragraph::new(month_year)
                        .block(Block::default().borders(Borders::ALL))
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(Color::Cyan));
                    f.render_widget(title, chunks[0]);

                    // Calculate financial data
                    let total = app.expense_manager.get_month_total(app.current_year, app.current_month);
                    let unpaid = app.expense_manager.get_unpaid_total(app.current_year, app.current_month);
                    let paid = total - unpaid;
                    let balance = app.expense_manager.get_balance();
                    let free_money = balance - unpaid;
                    let payment_progress = if total > 0.0 { (paid / total) * 100.0 } else { 100.0 };

                    // Create a layout for the financial summary
                    let summary_layout = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([
                            Constraint::Percentage(50),
                            Constraint::Percentage(50),
                        ].as_ref())
                        .split(chunks[2]);

                    // Left column - Balance and Free Money
                    let balance_style = Style::default().fg(Color::Green);
                    let free_money_style = if free_money >= 0.0 {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Red)
                    };

                    let left_text = vec![
                        format!("Current Balance"),
                        format!("{:.2} PLN", balance),
                        format!(""),
                        format!("Free Money"),
                        format!("{:.2} PLN", free_money),
                    ].join("\n");

                    let left_summary = Paragraph::new(left_text)
                        .block(Block::default().borders(Borders::ALL).title("Available Funds"))
                        .alignment(Alignment::Center)
                        .style(balance_style);

                    // Right column - Expenses Summary
                    let right_text = vec![
                        format!("Total Expenses"),
                        format!("{:.2} PLN", total),
                        format!(""),
                        format!("Paid: {:.2} PLN", paid),
                        format!("Unpaid: {:.2} PLN", unpaid),
                    ].join("\n");

                    let right_summary = Paragraph::new(right_text)
                        .block(Block::default().borders(Borders::ALL).title("Monthly Expenses"))
                        .alignment(Alignment::Center);

                    f.render_widget(left_summary, summary_layout[0]);
                    f.render_widget(right_summary, summary_layout[1]);

                    // Progress bar for payment progress
                    let progress_text = format!("Payment Progress: {:.1}%", payment_progress);
                    let progress_style = if payment_progress >= 80.0 {
                        Style::default().fg(Color::Green)
                    } else if payment_progress >= 50.0 {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Red)
                    };

                    let progress = Paragraph::new(progress_text)
                        .block(Block::default().borders(Borders::ALL))
                        .style(progress_style)
                        .alignment(Alignment::Center);

                    f.render_widget(progress, chunks[3]);

                    let help_text = "[h/l] Change Month | [b] Set Balance | [H] View History | [Esc] Back to List | [q] Quit";
                    let help = Paragraph::new(help_text)
                        .block(Block::default().borders(Borders::ALL))
                        .style(Style::default().fg(Color::Gray))
                        .alignment(Alignment::Center);
                    f.render_widget(help, chunks[5]);
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
                        .block(Block::default().borders(Borders::ALL));
                    f.render_widget(title, chunks[0]);

                    let history = app.expense_manager.get_balance_history();
                    let items: Vec<Row> = history.iter().map(|log| {
                        Row::new(vec![
                            log.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
                            format!("{:.2} PLN", log.balance),
                            format!("{:.2} PLN", log.total_expenses),
                            format!("{:.2} PLN", log.unpaid_expenses),
                            format!("{:.2} PLN", log.remaining),
                        ])
                    }).collect();

                    let table = Table::new(items)
                        .header(Row::new(vec![
                            "Timestamp",
                            "Balance",
                            "Total Expenses",
                            "Unpaid",
                            "Remaining"
                        ]).style(Style::default().fg(Color::Cyan)))
                        .block(Block::default().borders(Borders::ALL))
                        .widths(&[
                            Constraint::Percentage(25),
                            Constraint::Percentage(15),
                            Constraint::Percentage(20),
                            Constraint::Percentage(20),
                            Constraint::Percentage(20),
                        ]);

                    f.render_widget(table, chunks[1]);

                    let help_text = "[Esc] Back to Overview | [q] Quit";
                    let status = Paragraph::new(help_text)
                        .block(Block::default().borders(Borders::ALL));
                    f.render_widget(status, chunks[2]);
                }
                AppMode::AddBalance => {
                    let popup = centered_rect(40, 20, f.size());
                    let input = Paragraph::new(format!(
                        "Enter new balance amount:\n\nPLN {}\n\n[Enter] Save | [Esc] Cancel",
                        app.input_balance
                    ))
                    .block(Block::default().borders(Borders::ALL).title(" Set Balance "))
                    .alignment(Alignment::Center);

                    f.render_widget(Clear, popup);
                    f.render_widget(input, popup);
                }
            }

            if let InputMode::Adding(field) = app.input_mode {
                let popup = centered_rect(60, 30, f.size());
                let input_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Length(3),
                        Constraint::Length(2),
                    ].as_ref())
                    .split(popup);

                let desc_block = Block::default()
                    .borders(Borders::ALL)
                    .title(" Description ")
                    .border_style(if matches!(field, InputField::Description) {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    });

                let amount_block = Block::default()
                    .borders(Borders::ALL)
                    .title(" Amount (PLN) ")
                    .border_style(if matches!(field, InputField::Amount) {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    });

                let desc_text = Paragraph::new(app.input_description.as_str())
                    .block(desc_block);

                let amount_text = Paragraph::new(app.input_amount.as_str())
                    .block(amount_block);

                let help_text = Paragraph::new("[Tab] Switch Field | [Enter] Save | [Esc] Cancel")
                    .alignment(Alignment::Center);

                f.render_widget(Clear, popup);
                f.render_widget(desc_text, input_layout[0]);
                f.render_widget(amount_text, input_layout[1]);
                f.render_widget(help_text, input_layout[2]);
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
                        }
                    }
                    KeyCode::Char('o') => {
                        app.app_mode = AppMode::Overview;
                    }
                    KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('▼') => {
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
                    KeyCode::Char('k') | KeyCode::Up | KeyCode::Char('▲') => {
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
                    KeyCode::Char('h') | KeyCode::Left | KeyCode::Char('◄') => {
                        if app.current_month > 1 {
                            app.current_month -= 1;
                        } else {
                            app.current_month = 12;
                            app.current_year -= 1;
                        }
                        app.selected_expense = None;
                    }
                    KeyCode::Char('l') | KeyCode::Right | KeyCode::Char('►') => {
                        if app.current_month < 12 {
                            app.current_month += 1;
                        } else {
                            app.current_month = 1;
                            app.current_year += 1;
                        }
                        app.selected_expense = None;
                    }
                    KeyCode::Char(' ') => {
                        if matches!(app.app_mode, AppMode::List) {
                            if let Some(selected) = app.selected_expense {
                                if let Some(expenses) = app.expense_manager
                                    .get_month_expenses_mut(app.current_year, app.current_month)
                                {
                                    if let Some(expense) = expenses.get_mut(selected) {
                                        expense.toggle_paid();
                                        if let Err(e) = app.expense_manager.save() {
                                            log_message(&format!("Failed to save after toggling paid status: {}", e));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Char('d') => {
                        if matches!(app.app_mode, AppMode::List) {
                            if let Some(selected) = app.selected_expense {
                                if app.expense_manager.delete_expense(
                                    app.current_year,
                                    app.current_month,
                                    selected
                                ) {
                                    let new_len = app.expense_manager
                                        .get_month_expenses(app.current_year, app.current_month)
                                        .map(|e| e.len())
                                        .unwrap_or(0);
                                    
                                    if new_len == 0 {
                                        app.selected_expense = None;
                                    } else if selected >= new_len {
                                        app.selected_expense = Some(new_len - 1);
                                    }
                                    
                                    log_message(&format!("Deleted expense at index {}", selected));
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
                        if matches!(app.app_mode, AppMode::Overview) {
                            app.app_mode = AppMode::AddBalance;
                            app.input_balance.clear();
                        }
                    }
                    KeyCode::Esc => {
                        match app.app_mode {
                            AppMode::Overview => app.app_mode = AppMode::List,
                            AppMode::History => app.app_mode = AppMode::Overview,
                            AppMode::AddBalance => app.app_mode = AppMode::Overview,
                            _ => {}
                        }
                    }
                    KeyCode::Enter => {
                        if matches!(app.app_mode, AppMode::AddBalance) {
                            if let Ok(balance) = app.input_balance.parse::<f64>() {
                                app.expense_manager.set_balance(
                                    balance,
                                    app.current_year,
                                    app.current_month
                                );
                                if let Err(e) = app.expense_manager.save() {
                                    log_message(&format!("Failed to save after setting balance: {}", e));
                                }
                                app.app_mode = AppMode::Overview;
                            }
                        }
                    }
                    KeyCode::Char(c) => {
                        if matches!(app.app_mode, AppMode::AddBalance) {
                            if c.is_digit(10) || c == '.' {
                                app.input_balance.push(c);
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
                            InputField::Amount => InputField::Description,
                        });
                    }
                    KeyCode::Enter => {
                        if !app.input_description.is_empty() && !app.input_amount.is_empty() {
                            if let Ok(amount) = app.input_amount.parse::<f64>() {
                                let expense = Expense::new(
                                    app.input_description.clone(),
                                    amount,
                                );
                                app.expense_manager.add_expense(
                                    app.current_year,
                                    app.current_month,
                                    expense,
                                );
                                if let Err(e) = app.expense_manager.save() {
                                    log_message(&format!("Failed to save after adding expense: {}", e));
                                }
                                app.input_mode = InputMode::Normal;
                                app.input_description.clear();
                                app.input_amount.clear();
                            }
                        }
                    }
                    KeyCode::Char(c) => {
                        match field {
                            InputField::Description => {
                                app.input_description.push(c);
                            }
                            InputField::Amount => {
                                if c.is_digit(10) || c == '.' {
                                    app.input_amount.push(c);
                                }
                            }
                        }
                    }
                    KeyCode::Backspace => {
                        match field {
                            InputField::Description => {
                                app.input_description.pop();
                            }
                            InputField::Amount => {
                                app.input_amount.pop();
                            }
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
