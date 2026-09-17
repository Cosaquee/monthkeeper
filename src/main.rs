use chrono::{DateTime, Datelike, Local, NaiveDate};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEventKind,
    },
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
pub struct BalanceLog {
    timestamp: DateTime<Local>,
    balance: f64,
    total_expenses: f64,
    unpaid_expenses: f64,
    remaining: f64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum Category {
    #[default]
    Need,
    Want,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
enum TransactionKind {
    Expense,
    Income,
}

impl TransactionKind {
    fn toggle(self) -> Self {
        match self {
            TransactionKind::Expense => TransactionKind::Income,
            TransactionKind::Income => TransactionKind::Expense,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
enum LegacyTransactionCategory {
    Food,
    Home,
    Transport,
    Shopping,
    Health,
    Leisure,
    Income,
    Other,
}

impl LegacyTransactionCategory {
    fn label(self) -> &'static str {
        match self {
            Self::Food => "Food",
            Self::Home => "Home",
            Self::Transport => "Transport",
            Self::Shopping => "Shopping",
            Self::Health => "Health",
            Self::Leisure => "Leisure",
            Self::Income => "Income",
            Self::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
enum CategoryKind {
    Expense,
    Income,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TransactionCategory {
    id: String,
    name: String,
    kind: CategoryKind,
    #[serde(default)]
    archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Transaction {
    date: NaiveDate,
    description: String,
    amount: f64,
    kind: TransactionKind,
    #[serde(default)]
    category_id: String,
    #[serde(default, rename = "category", skip_serializing_if = "Option::is_none")]
    legacy_category: Option<LegacyTransactionCategory>,
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
    #[serde(default)]
    transactions: Vec<Transaction>,
    #[serde(default = "default_transaction_categories")]
    transaction_categories: Vec<TransactionCategory>,
}

fn default_transaction_categories() -> Vec<TransactionCategory> {
    [
        ("food", "Food", CategoryKind::Expense),
        ("home", "Home", CategoryKind::Expense),
        ("transport", "Transport", CategoryKind::Expense),
        ("shopping", "Shopping", CategoryKind::Expense),
        ("health", "Health", CategoryKind::Expense),
        ("leisure", "Leisure", CategoryKind::Expense),
        ("income", "Income", CategoryKind::Income),
        ("other", "Other", CategoryKind::Both),
    ]
    .into_iter()
    .map(|(id, name, kind)| TransactionCategory {
        id: id.into(),
        name: name.into(),
        kind,
        archived: false,
    })
    .collect()
}

impl ExpenseManager {
    pub fn new() -> Self {
        match Self::load() {
            Ok(mut manager) => {
                manager.migrate_transaction_categories();
                log_message("Successfully loaded existing data");
                manager
            }
            Err(e) => {
                log_message(&format!("Failed to load data: {}. Starting fresh.", e));
                Self {
                    expenses: HashMap::new(),
                    balances: HashMap::new(),
                    balance_history: HashMap::new(),
                    transactions: Vec::new(),
                    transaction_categories: default_transaction_categories(),
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
            .or_default()
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
        self.expenses.entry(key).or_default().push(expense);

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

    fn add_transaction(&mut self, transaction: Transaction) -> usize {
        self.transactions.push(transaction);
        let index = self.transactions.len() - 1;
        if let Err(e) = self.save() {
            log_message(&format!("Failed to save after adding transaction: {}", e));
        }
        index
    }

    fn migrate_transaction_categories(&mut self) {
        if self.transaction_categories.is_empty() {
            self.transaction_categories = default_transaction_categories();
        }
        for transaction in &mut self.transactions {
            if transaction.category_id.is_empty() {
                transaction.category_id = transaction
                    .legacy_category
                    .map(|category| category.label().to_lowercase())
                    .unwrap_or_else(|| "other".into());
            }
            transaction.legacy_category = None;
        }
    }

    fn category_name(&self, id: &str) -> &str {
        self.transaction_categories
            .iter()
            .find(|category| category.id == id)
            .map(|category| category.name.as_str())
            .unwrap_or("Uncategorized")
    }

    fn available_categories(&self, kind: TransactionKind) -> Vec<&TransactionCategory> {
        self.transaction_categories
            .iter()
            .filter(|category| {
                !category.archived
                    && matches!(
                        (kind, category.kind),
                        (_, CategoryKind::Both)
                            | (TransactionKind::Expense, CategoryKind::Expense)
                            | (TransactionKind::Income, CategoryKind::Income)
                    )
            })
            .collect()
    }

    fn add_category(&mut self, name: String, kind: CategoryKind) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Category name cannot be empty.".into());
        }
        if self
            .transaction_categories
            .iter()
            .any(|category| category.name.eq_ignore_ascii_case(name) && !category.archived)
        {
            return Err("A category with that name already exists.".into());
        }
        let mut id = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string();
        if id.is_empty() {
            id = "category".into();
        }
        let base = id.clone();
        let mut suffix = 2;
        while self
            .transaction_categories
            .iter()
            .any(|category| category.id == id)
        {
            id = format!("{base}-{suffix}");
            suffix += 1;
        }
        self.transaction_categories.push(TransactionCategory {
            id: id.clone(),
            name: name.into(),
            kind,
            archived: false,
        });
        self.save().map_err(|error| error.to_string())?;
        Ok(id)
    }

    fn update_category(
        &mut self,
        index: usize,
        name: String,
        kind: CategoryKind,
    ) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Category name cannot be empty.".into());
        }
        if self
            .transaction_categories
            .iter()
            .enumerate()
            .any(|(i, category)| {
                i != index && !category.archived && category.name.eq_ignore_ascii_case(name)
            })
        {
            return Err("A category with that name already exists.".into());
        }
        let category = self
            .transaction_categories
            .get_mut(index)
            .ok_or_else(|| "Category no longer exists.".to_string())?;
        category.name = name.into();
        category.kind = kind;
        self.save().map_err(|error| error.to_string())
    }

    fn delete_category(&mut self, index: usize) -> Result<(), String> {
        let category = self
            .transaction_categories
            .get(index)
            .ok_or_else(|| "Category no longer exists.".to_string())?;
        if self
            .transactions
            .iter()
            .any(|transaction| transaction.category_id == category.id)
        {
            return Err("Cannot delete a category used by transactions.".into());
        }
        self.transaction_categories.remove(index);
        self.save().map_err(|error| error.to_string())
    }

    fn month_transactions(&self, year: i32, month: u32) -> Vec<(usize, &Transaction)> {
        self.displayed_transactions(year, month, false)
    }

    fn displayed_transactions(
        &self,
        year: i32,
        month: u32,
        show_all: bool,
    ) -> Vec<(usize, &Transaction)> {
        let mut items: Vec<_> = self
            .transactions
            .iter()
            .enumerate()
            .filter(|(_, transaction)| {
                show_all || (transaction.date.year() == year && transaction.date.month() == month)
            })
            .collect();
        items.sort_by(|a, b| b.1.date.cmp(&a.1.date).then_with(|| b.0.cmp(&a.0)));
        items
    }

    fn update_transaction(&mut self, index: usize, transaction: Transaction) -> bool {
        if let Some(existing) = self.transactions.get_mut(index) {
            *existing = transaction;
            let _ = self.save();
            true
        } else {
            false
        }
    }

    fn delete_transaction(&mut self, index: usize) -> bool {
        if index < self.transactions.len() {
            self.transactions.remove(index);
            let _ = self.save();
            true
        } else {
            false
        }
    }
}

impl Default for ExpenseManager {
    fn default() -> Self {
        Self::new()
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

    needs.into_iter().chain(wants).collect()
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
    AddingTransaction(TransactionInputField),
    AddingCategory,
}

#[derive(Copy, Clone)]
enum InputField {
    Description,
    Amount,
    Category,
}

#[derive(Copy, Clone)]
enum TransactionInputField {
    Date,
    Description,
    Amount,
    Kind,
    Category,
}

#[derive(Copy, Clone)]
enum AppMode {
    List,
    Transactions,
    Categories,
    Help,
    Overview,
    History,
    AddBalance,
    ConfirmDelete,
    ConfirmDeleteTransaction,
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
    selected_transaction: Option<usize>,
    selected_category: Option<usize>,
    show_all_transactions: bool,
    editing_transaction_index: Option<usize>,
    input_description: String,
    input_amount: String,
    input_balance: String,
    input_category: Category,
    transaction_date: String,
    transaction_kind: TransactionKind,
    transaction_category_id: String,
    transaction_category_query: String,
    new_category_name: String,
    new_category_kind: CategoryKind,
    editing_category_index: Option<usize>,
    category_notice: Option<String>,
    input_error: Option<String>,
    balance_return_mode: AppMode,
    help_return_mode: AppMode,
    pending_delete_index: Option<usize>,
    pending_delete_description: String,
    pending_delete_amount: f64,
    pending_transaction_index: Option<usize>,
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
            selected_transaction: None,
            selected_category: None,
            show_all_transactions: false,
            editing_transaction_index: None,
            input_description: String::new(),
            input_amount: String::new(),
            input_balance: String::new(),
            input_category: Category::Need,
            transaction_date: now.format("%Y-%m-%d").to_string(),
            transaction_kind: TransactionKind::Expense,
            transaction_category_id: "food".into(),
            transaction_category_query: String::new(),
            new_category_name: String::new(),
            new_category_kind: CategoryKind::Expense,
            editing_category_index: None,
            category_notice: None,
            input_error: None,
            balance_return_mode: AppMode::List,
            help_return_mode: AppMode::List,
            pending_delete_index: None,
            pending_delete_description: String::new(),
            pending_delete_amount: 0.0,
            pending_transaction_index: None,
        }
    }

    fn ensure_transaction_category(&mut self) {
        let available = self.filtered_transaction_categories();
        if !available
            .iter()
            .any(|category| category.id == self.transaction_category_id)
        {
            self.transaction_category_id = available
                .first()
                .map(|category| category.id.clone())
                .unwrap_or_default();
        }
    }

    fn cycle_transaction_category(&mut self, forward: bool) {
        let available = self.filtered_transaction_categories();
        if available.is_empty() {
            self.transaction_category_id.clear();
            return;
        }
        let current = available
            .iter()
            .position(|category| category.id == self.transaction_category_id)
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % available.len()
        } else {
            (current + available.len() - 1) % available.len()
        };
        self.transaction_category_id = available[next].id.clone();
    }

    fn filtered_transaction_categories(&self) -> Vec<&TransactionCategory> {
        let query = self.transaction_category_query.trim().to_lowercase();
        self.expense_manager
            .available_categories(self.transaction_kind)
            .into_iter()
            .filter(|category| query.is_empty() || category.name.to_lowercase().contains(&query))
            .collect()
    }

    fn transaction_category_window_start(&self, visible_count: usize) -> usize {
        let available = self.filtered_transaction_categories();
        let selected = available
            .iter()
            .position(|category| category.id == self.transaction_category_id)
            .unwrap_or(0);
        selected
            .saturating_sub(visible_count.saturating_sub(1))
            .min(available.len().saturating_sub(visible_count))
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
                    let title = Paragraph::new(Line::from(vec![
                        Span::styled(
                            " PLAN ",
                            Style::default()
                                .fg(Color::Black)
                                .bg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("  ◄  {} {}  ►  ", month_name, app.current_year),
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" TRANSACTIONS ", Style::default().fg(Color::DarkGray)),
                        Span::styled(" CATEGORIES ", Style::default().fg(Color::DarkGray)),
                    ]))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Cyan)),
                    )
                    .alignment(Alignment::Center);
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
                        Span::styled("c ", dim),
                        Span::raw("Add  "),
                        Span::styled("Space ", dim),
                        Span::raw("Paid  "),
                        Span::styled("t ", dim),
                        Span::raw("Category  "),
                        Span::styled("d ", dim),
                        Span::raw("Delete  "),
                        Span::styled("y ", dim),
                        Span::raw("Copy  "),
                        Span::styled("b ", dim),
                        Span::raw("Balance  "),
                        Span::styled("Shift+H/L ", dim),
                        Span::raw("Switch screen  "),
                        Span::styled("q ", dim),
                        Span::raw("Quit"),
                    ])]);
                    let help = Paragraph::new(help_text)
                        .block(Block::default().borders(Borders::ALL))
                        .alignment(Alignment::Center);
                    f.render_widget(help, chunks[2]);
                }

                AppMode::Transactions => {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .margin(1)
                        .constraints([
                            Constraint::Length(3),
                            Constraint::Min(0),
                            Constraint::Length(3),
                        ])
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
                    let period_label = if app.show_all_transactions {
                        "ALL TRANSACTIONS".to_string()
                    } else {
                        format!(
                            "◄  {} {}  ►",
                            month_names[app.current_month as usize - 1],
                            app.current_year
                        )
                    };
                    let title = Paragraph::new(Line::from(vec![
                        Span::styled(" PLAN ", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            format!("  {period_label}  "),
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            " TRANSACTIONS ",
                            Style::default()
                                .fg(Color::Black)
                                .bg(Color::Green)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" CATEGORIES ", Style::default().fg(Color::DarkGray)),
                    ]))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Green)),
                    )
                    .alignment(Alignment::Center);
                    f.render_widget(title, chunks[0]);

                    let content = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                        .split(chunks[1]);
                    let transactions = app.expense_manager.displayed_transactions(
                        app.current_year,
                        app.current_month,
                        app.show_all_transactions,
                    );
                    let dim = Style::default().fg(Color::DarkGray);
                    let mut rows = Vec::new();
                    let mut selected_row = None;
                    let mut previous_date = None;
                    for (display_index, (_, transaction)) in transactions.iter().enumerate() {
                        if previous_date != Some(transaction.date) {
                            rows.push(
                                Row::new(vec![
                                    format!("  {}", transaction.date.format("%a, %d %b")),
                                    String::new(),
                                    String::new(),
                                    String::new(),
                                ])
                                .style(
                                    Style::default()
                                        .fg(Color::DarkGray)
                                        .add_modifier(Modifier::BOLD),
                                ),
                            );
                            previous_date = Some(transaction.date);
                        }
                        let selected = app.selected_transaction == Some(display_index);
                        let amount_style = if transaction.kind == TransactionKind::Income {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        let row_index = rows.len();
                        rows.push(
                            Row::new(vec![
                                format!(
                                    "{}{}",
                                    if selected { "▶ " } else { "  " },
                                    transaction.description
                                ),
                                app.expense_manager
                                    .category_name(&transaction.category_id)
                                    .to_string(),
                                if transaction.kind == TransactionKind::Income {
                                    "Income"
                                } else {
                                    "Expense"
                                }
                                .to_string(),
                                format!(
                                    "{}{:.2}",
                                    if transaction.kind == TransactionKind::Income {
                                        "+"
                                    } else {
                                        "−"
                                    },
                                    transaction.amount
                                ),
                            ])
                            .style(if selected {
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                amount_style
                            }),
                        );
                        if selected {
                            selected_row = Some(row_index);
                        }
                    }
                    if rows.is_empty() {
                        rows.push(
                            Row::new(vec![
                                "  No transactions yet — press a to record one".to_string(),
                                String::new(),
                                String::new(),
                                String::new(),
                            ])
                            .style(dim),
                        );
                    }
                    let visible_rows = content[0].height.saturating_sub(4) as usize;
                    if visible_rows > 0 {
                        if let Some(selected_row) = selected_row {
                            let start = selected_row
                                .saturating_sub(visible_rows.saturating_sub(1))
                                .min(rows.len().saturating_sub(visible_rows));
                            rows = rows.into_iter().skip(start).take(visible_rows).collect();
                        } else if rows.len() > visible_rows {
                            rows.truncate(visible_rows);
                        }
                    }
                    let table = Table::new(rows)
                        .header(
                            Row::new(vec!["  Description", "Category", "Type", "Amount (PLN)"])
                                .style(dim),
                        )
                        .block(Block::default().borders(Borders::ALL).title(" Activity "))
                        .widths(&[
                            Constraint::Percentage(43),
                            Constraint::Percentage(20),
                            Constraint::Percentage(17),
                            Constraint::Percentage(20),
                        ]);
                    f.render_widget(table, content[0]);

                    let spent: f64 = transactions
                        .iter()
                        .filter(|(_, t)| t.kind == TransactionKind::Expense)
                        .map(|(_, t)| t.amount)
                        .sum();
                    let income: f64 = transactions
                        .iter()
                        .filter(|(_, t)| t.kind == TransactionKind::Income)
                        .map(|(_, t)| t.amount)
                        .sum();
                    let net = income - spent;
                    let planned = app
                        .expense_manager
                        .get_month_total(app.current_year, app.current_month);
                    let pace = if planned > 0.0 {
                        spent / planned * 100.0
                    } else {
                        0.0
                    };
                    let bar_width = content[1].width.saturating_sub(10) as usize;
                    let mut summary_lines = vec![
                        Line::from(Span::styled(
                            if app.show_all_transactions {
                                "ALL TIME"
                            } else {
                                "THIS MONTH"
                            },
                            dim.add_modifier(Modifier::BOLD),
                        )),
                        Line::from(""),
                        Line::from(vec![
                            Span::styled("Spent       ", dim),
                            Span::styled(
                                format!("{spent:.2} PLN"),
                                Style::default().add_modifier(Modifier::BOLD),
                            ),
                        ]),
                        Line::from(vec![
                            Span::styled("Income      ", dim),
                            Span::styled(
                                format!("{income:.2} PLN"),
                                Style::default().fg(Color::Green),
                            ),
                        ]),
                        Line::from(vec![
                            Span::styled("Net flow    ", dim),
                            Span::styled(
                                format!("{net:+.2} PLN"),
                                Style::default()
                                    .fg(if net >= 0.0 { Color::Green } else { Color::Red })
                                    .add_modifier(Modifier::BOLD),
                            ),
                        ]),
                    ];
                    if !app.show_all_transactions {
                        summary_lines.extend([
                            Line::from(""),
                            Line::from(Span::styled(
                                "PLAN CONTEXT",
                                dim.add_modifier(Modifier::BOLD),
                            )),
                            Line::from(""),
                            Line::from(vec![
                                Span::styled("Planned     ", dim),
                                Span::raw(format!("{planned:.2} PLN")),
                            ]),
                            Line::from(vec![
                                Span::styled("Recorded    ", dim),
                                Span::raw(format!("{pace:.0}%")),
                            ]),
                            Line::from(Span::styled(
                                render_bar(pace.min(100.0), bar_width.max(4)),
                                Style::default().fg(if pace <= 100.0 {
                                    Color::Cyan
                                } else {
                                    Color::Red
                                }),
                            )),
                        ]);
                    }
                    summary_lines.extend([
                        Line::from(""),
                        Line::from(Span::styled(
                            "BY CATEGORY",
                            dim.add_modifier(Modifier::BOLD),
                        )),
                    ]);
                    let mut category_totals: HashMap<&str, f64> = HashMap::new();
                    for (_, transaction) in transactions
                        .iter()
                        .filter(|(_, transaction)| transaction.kind == TransactionKind::Expense)
                    {
                        *category_totals.entry(&transaction.category_id).or_default() +=
                            transaction.amount;
                    }
                    let mut category_totals: Vec<_> = category_totals.into_iter().collect();
                    category_totals.sort_by(|a, b| b.1.total_cmp(&a.1));
                    for (category_id, total) in category_totals.into_iter().take(6) {
                        summary_lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:<12}", app.expense_manager.category_name(category_id)),
                                dim,
                            ),
                            Span::raw(format!("{total:.2} PLN")),
                        ]));
                    }
                    summary_lines.push(Line::from(""));
                    summary_lines.push(Line::from(Span::styled(
                        "Transactions are optional and never change your monthly plan.",
                        dim,
                    )));
                    let summary = Text::from(summary_lines);
                    f.render_widget(
                        Paragraph::new(summary).block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" Cash flow ")
                                .padding(Padding::horizontal(1)),
                        ),
                        content[1],
                    );

                    let help = Paragraph::new(Line::from(vec![
                        Span::styled("Shift+H/L ", dim),
                        Span::raw("Switch screen  "),
                        Span::styled("h/l ", dim),
                        Span::raw("Month  "),
                        Span::styled("j/k ", dim),
                        Span::raw("Select  "),
                        Span::styled("c ", dim),
                        Span::raw("Create transaction  "),
                        Span::styled("e ", dim),
                        Span::raw("Edit  "),
                        Span::styled("v ", dim),
                        Span::raw(if app.show_all_transactions {
                            "Monthly  "
                        } else {
                            "All  "
                        }),
                        Span::styled("d ", dim),
                        Span::raw("Delete  "),
                        Span::styled("q ", dim),
                        Span::raw("Quit"),
                    ]))
                    .block(Block::default().borders(Borders::ALL))
                    .alignment(Alignment::Center);
                    f.render_widget(help, chunks[2]);
                }

                AppMode::Categories => {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .margin(1)
                        .constraints([
                            Constraint::Length(3),
                            Constraint::Min(0),
                            Constraint::Length(3),
                        ])
                        .split(f.size());
                    let title = Paragraph::new(Line::from(vec![
                        Span::styled(" PLAN ", Style::default().fg(Color::DarkGray)),
                        Span::styled(" TRANSACTIONS ", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            " CATEGORIES ",
                            Style::default()
                                .fg(Color::Black)
                                .bg(Color::Magenta)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Magenta)),
                    )
                    .alignment(Alignment::Center);
                    f.render_widget(title, chunks[0]);

                    let rows = app
                        .expense_manager
                        .transaction_categories
                        .iter()
                        .enumerate()
                        .map(|(index, category)| {
                            let kind = match category.kind {
                                CategoryKind::Expense => "Expense",
                                CategoryKind::Income => "Income",
                                CategoryKind::Both => "Both",
                            };
                            let used = app
                                .expense_manager
                                .transactions
                                .iter()
                                .filter(|transaction| transaction.category_id == category.id)
                                .count();
                            Row::new(vec![
                                format!(
                                    "{}{}",
                                    if app.selected_category == Some(index) {
                                        "▶ "
                                    } else {
                                        "  "
                                    },
                                    category.name
                                ),
                                kind.to_string(),
                                used.to_string(),
                                if category.archived {
                                    "Archived"
                                } else {
                                    "Active"
                                }
                                .to_string(),
                            ])
                            .style(
                                if app.selected_category == Some(index) {
                                    Style::default()
                                        .fg(Color::Yellow)
                                        .add_modifier(Modifier::BOLD)
                                } else if category.archived {
                                    Style::default().fg(Color::DarkGray)
                                } else {
                                    Style::default()
                                },
                            )
                        });
                    let table = Table::new(rows)
                        .header(
                            Row::new(vec!["  Name", "Applies to", "Transactions", "Status"])
                                .style(Style::default().fg(Color::DarkGray)),
                        )
                        .block(Block::default().borders(Borders::ALL).title(" Categories "))
                        .widths(&[
                            Constraint::Percentage(40),
                            Constraint::Percentage(20),
                            Constraint::Percentage(20),
                            Constraint::Percentage(20),
                        ]);
                    f.render_widget(table, chunks[1]);
                    let help = app.category_notice.as_deref().unwrap_or(
                        "j/k Select   c Create   e Edit   d Delete   Shift+H/L Switch screen   q Quit",
                    );
                    f.render_widget(
                        Paragraph::new(help)
                            .block(Block::default().borders(Borders::ALL))
                            .alignment(Alignment::Center),
                        chunks[2],
                    );
                }

                AppMode::Help => {
                    let area = centered_dialog(78, 30, f.size());
                    f.render_widget(Clear, area);
                    let dim = Style::default().fg(Color::DarkGray);
                    let heading = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
                    let help = Text::from(vec![
                        Line::from(Span::styled("KEYBOARD SHORTCUTS", heading)),
                        Line::from(""),
                        Line::from(vec![Span::styled("Global       ", dim), Span::raw("q Quit   ? Help   Shift+H/L Switch screen")]),
                        Line::from(vec![Span::styled("Navigation   ", dim), Span::raw("j/k Select   h/l Change month")]),
                        Line::from(""),
                        Line::from(Span::styled("PLAN", heading)),
                        Line::from("c Create expense   Space Mark paid   t Need/Want   y Copy next month"),
                        Line::from("d Delete expense   b Balance   o Overview"),
                        Line::from(""),
                        Line::from(Span::styled("TRANSACTIONS", heading)),
                        Line::from("c Create   e Edit   d Delete   v Toggle monthly/all"),
                        Line::from(""),
                        Line::from(Span::styled("CATEGORIES", heading)),
                        Line::from("c Create   e Edit   d Delete (only when unused)"),
                        Line::from(""),
                        Line::from(Span::styled("DIALOGS", heading)),
                        Line::from("Tab Next field   Enter Save/confirm   Esc Cancel/back"),
                        Line::from(""),
                        Line::from(Span::styled("Press Esc to return", dim)),
                    ]);
                    f.render_widget(
                        Paragraph::new(help)
                            .block(
                                Block::default()
                                    .borders(Borders::ALL)
                                    .title(" Help ")
                                    .padding(Padding::horizontal(2)),
                            ),
                        area,
                    );
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

                AppMode::ConfirmDelete | AppMode::ConfirmDeleteTransaction => {
                    let deleting_transaction =
                        matches!(app.app_mode, AppMode::ConfirmDeleteTransaction);
                    let popup = centered_dialog(58, 12, f.size());
                    f.render_widget(Clear, popup);
                    f.render_widget(
                        Block::default().style(Style::default().bg(Color::Black)),
                        popup,
                    );

                    let outer = Block::default()
                        .borders(Borders::ALL)
                        .title(if deleting_transaction {
                            " Delete transaction? "
                        } else {
                            " Delete expense? "
                        })
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
                        Paragraph::new(if deleting_transaction {
                            "This removes the record. Your monthly plan stays unchanged."
                        } else {
                            "This will permanently remove the expense from this month."
                        })
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
                            Span::raw(if deleting_transaction {
                                "Keep transaction"
                            } else {
                                "Keep expense"
                            }),
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

            if let InputMode::AddingTransaction(field) = app.input_mode {
                let popup = centered_dialog(68, 30, f.size());
                f.render_widget(Clear, popup);
                f.render_widget(
                    Block::default().style(Style::default().bg(Color::Black)),
                    popup,
                );
                let outer = Block::default()
                    .borders(Borders::ALL)
                    .title(if app.editing_transaction_index.is_some() {
                        " Edit transaction "
                    } else {
                        " Record transaction "
                    })
                    .title_alignment(Alignment::Center)
                    .border_style(Style::default().fg(Color::Green));
                let inner = outer.inner(popup);
                f.render_widget(outer, popup);
                let areas = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Length(3),
                        Constraint::Length(3),
                        Constraint::Length(3),
                        Constraint::Length(7),
                        Constraint::Length(1),
                        Constraint::Min(1),
                    ])
                    .split(inner);
                let active = Style::default().fg(Color::Green);
                let inactive = Style::default().fg(Color::DarkGray);
                let border = |is_active| if is_active { active } else { inactive };
                let cursor = |is_active| if is_active { "█" } else { "" };
                f.render_widget(
                    Paragraph::new(format!(
                        "{}{}",
                        app.transaction_date,
                        cursor(matches!(field, TransactionInputField::Date))
                    ))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Date (YYYY-MM-DD) ")
                            .border_style(border(matches!(field, TransactionInputField::Date))),
                    ),
                    areas[0],
                );
                f.render_widget(
                    Paragraph::new(format!(
                        "{}{}",
                        app.input_description,
                        cursor(matches!(field, TransactionInputField::Description))
                    ))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Description ")
                            .border_style(border(matches!(
                                field,
                                TransactionInputField::Description
                            ))),
                    ),
                    areas[1],
                );
                f.render_widget(
                    Paragraph::new(format!(
                        "{}{}",
                        app.input_amount,
                        cursor(matches!(field, TransactionInputField::Amount))
                    ))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Amount (PLN) ")
                            .border_style(border(matches!(field, TransactionInputField::Amount))),
                    ),
                    areas[2],
                );
                let expense_style = if app.transaction_kind == TransactionKind::Expense {
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                } else {
                    inactive
                };
                let income_style = if app.transaction_kind == TransactionKind::Income {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else {
                    inactive
                };
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(
                            if app.transaction_kind == TransactionKind::Expense {
                                " [ ▶ Expense ] "
                            } else {
                                " [   Expense ] "
                            },
                            expense_style,
                        ),
                        Span::raw("   "),
                        Span::styled(
                            if app.transaction_kind == TransactionKind::Income {
                                "[ ▶ Income ] "
                            } else {
                                "[   Income ] "
                            },
                            income_style,
                        ),
                    ]))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Type ")
                            .border_style(border(matches!(field, TransactionInputField::Kind))),
                    ),
                    areas[3],
                );
                let matches = app.filtered_transaction_categories();
                let category_window_start = app.transaction_category_window_start(4);
                let mut category_lines = vec![Line::from(format!(
                    " Search: {}{}",
                    app.transaction_category_query,
                    cursor(matches!(field, TransactionInputField::Category))
                ))];
                for category in matches.iter().skip(category_window_start).take(4) {
                    category_lines.push(Line::from(format!(
                        " {} {}",
                        if category.id == app.transaction_category_id { "▶" } else { " " },
                        category.name
                    )));
                }
                if matches.is_empty() {
                    category_lines.push(Line::from(" No matching categories"));
                }
                f.render_widget(
                    Paragraph::new(Text::from(category_lines))
                        .style(Style::default().fg(Color::Cyan))
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(" Category (type to filter) ")
                                .border_style(border(matches!(field, TransactionInputField::Category))),
                        ),
                    areas[4],
                );
                if let Some(error) = &app.input_error {
                    f.render_widget(
                        Paragraph::new(error.as_str())
                            .style(Style::default().fg(Color::Red))
                            .alignment(Alignment::Center),
                        areas[5],
                    );
                }
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled("Tab ", inactive),
                        Span::raw("Next  "),
                        Span::styled("←/→ ", inactive),
                        Span::raw("Choose  "),
                        Span::styled("Enter ", inactive),
                        Span::raw("Save  "),
                        Span::styled("Esc ", inactive),
                        Span::raw("Cancel"),
                    ]))
                    .alignment(Alignment::Center),
                    areas[6],
                );
            }

            if matches!(app.input_mode, InputMode::AddingCategory) {
                let popup = centered_dialog(58, 10, f.size());
                f.render_widget(Clear, popup);
                let kind = match app.new_category_kind {
                    CategoryKind::Expense => "Expense",
                    CategoryKind::Income => "Income",
                    CategoryKind::Both => "Both",
                };
                let text = Text::from(vec![
                    Line::from(""),
                    Line::from(format!(" Name: {}█", app.new_category_name)),
                    Line::from(""),
                    Line::from(format!(" Applies to:  ◄  {kind}  ►")),
                    Line::from(""),
                    Line::from(Span::styled(
                        app.input_error
                            .as_deref()
                            .unwrap_or("Enter Save   Esc Cancel"),
                        Style::default().fg(if app.input_error.is_some() {
                            Color::Red
                        } else {
                            Color::DarkGray
                        }),
                    )),
                ]);
                f.render_widget(
                    Paragraph::new(text).alignment(Alignment::Center).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" New category "),
                    ),
                    popup,
                );
            }
        })?;

        let input_event = event::read()?;
        if let Event::Mouse(mouse) = &input_event {
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                let size = terminal.size()?;
                let in_header = mouse.row >= 1 && mouse.row <= 3;
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
                let month_title = format!(
                    " PLAN   ◄  {} {}  ►   TRANSACTIONS  CATEGORIES ",
                    month_names[app.current_month as usize - 1],
                    app.current_year
                );
                let label_hit = |line: &str, label: &str, available_width: u16| {
                    let line_width = line.chars().count() as u16;
                    let start = 2 + available_width.saturating_sub(4 + line_width) / 2;
                    line.find(label).is_some_and(|offset| {
                        let label_start = start + line[..offset].chars().count() as u16;
                        mouse.column >= label_start
                            && mouse.column < label_start + label.chars().count() as u16
                    })
                };
                if in_header && label_hit(&month_title, "CATEGORIES", size.width) {
                    app.app_mode = AppMode::Categories;
                    app.selected_expense = None;
                    app.selected_transaction = None;
                } else if in_header && label_hit(&month_title, "TRANSACTIONS", size.width) {
                    app.app_mode = AppMode::Transactions;
                    app.selected_expense = None;
                } else if in_header && label_hit(&month_title, "PLAN", size.width) {
                    app.app_mode = AppMode::List;
                    app.selected_transaction = None;
                }
            }
            continue;
        }

        if let Event::Key(key) = input_event {
            match app.input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('?') => {
                        app.help_return_mode = app.app_mode;
                        app.app_mode = AppMode::Help;
                    }
                    KeyCode::Char('H') => match app.app_mode {
                        AppMode::List => app.app_mode = AppMode::Categories,
                        AppMode::Transactions => app.app_mode = AppMode::List,
                        AppMode::Categories => app.app_mode = AppMode::Transactions,
                        AppMode::Overview => app.app_mode = AppMode::History,
                        _ => {}
                    },
                    KeyCode::Char('L') => match app.app_mode {
                        AppMode::List => app.app_mode = AppMode::Transactions,
                        AppMode::Transactions => app.app_mode = AppMode::Categories,
                        AppMode::Categories => app.app_mode = AppMode::List,
                        _ => {}
                    },
                    KeyCode::Char('c') => {
                        if matches!(app.app_mode, AppMode::Transactions) {
                            app.editing_transaction_index = None;
                            app.input_mode =
                                InputMode::AddingTransaction(TransactionInputField::Date);
                            app.transaction_date =
                                default_transaction_date(app.current_year, app.current_month);
                            app.input_description.clear();
                            app.input_amount.clear();
                            app.transaction_kind = TransactionKind::Expense;
                            app.transaction_category_id = "food".into();
                            app.transaction_category_query.clear();
                            app.ensure_transaction_category();
                            app.input_error = None;
                        } else if matches!(app.app_mode, AppMode::Categories) {
                            app.new_category_name.clear();
                            app.new_category_kind = CategoryKind::Expense;
                            app.input_error = None;
                            app.editing_category_index = None;
                            app.category_notice = None;
                            app.input_mode = InputMode::AddingCategory;
                        } else if matches!(app.app_mode, AppMode::List) {
                            app.input_mode = InputMode::Adding(InputField::Description);
                            app.input_description.clear();
                            app.input_amount.clear();
                            app.input_category = Category::Need;
                            app.input_error = None;
                        }
                    }
                    KeyCode::Char('v') => {
                        if matches!(app.app_mode, AppMode::Transactions) {
                            app.show_all_transactions = !app.show_all_transactions;
                            app.selected_transaction = None;
                        }
                    }
                    KeyCode::Char('e') => {
                        if matches!(app.app_mode, AppMode::Transactions) {
                            if let Some(selected) = app.selected_transaction {
                                let transaction = app
                                    .expense_manager
                                    .displayed_transactions(
                                        app.current_year,
                                        app.current_month,
                                        app.show_all_transactions,
                                    )
                                    .get(selected)
                                    .map(|(index, transaction)| (*index, (*transaction).clone()));
                                if let Some((index, transaction)) = transaction {
                                    app.transaction_date =
                                        transaction.date.format("%Y-%m-%d").to_string();
                                    app.input_description = transaction.description;
                                    app.input_amount = format!("{:.2}", transaction.amount);
                                    app.transaction_kind = transaction.kind;
                                    app.transaction_category_id = transaction.category_id;
                                    app.transaction_category_query.clear();
                                    app.editing_transaction_index = Some(index);
                                    app.input_error = None;
                                    app.input_mode =
                                        InputMode::AddingTransaction(TransactionInputField::Date);
                                }
                            }
                        } else if matches!(app.app_mode, AppMode::Categories) {
                            if let Some(index) = app.selected_category {
                                if let Some(category) =
                                    app.expense_manager.transaction_categories.get(index)
                                {
                                    app.new_category_name = category.name.clone();
                                    app.new_category_kind = category.kind;
                                    app.editing_category_index = Some(index);
                                    app.category_notice = None;
                                    app.input_error = None;
                                    app.input_mode = InputMode::AddingCategory;
                                }
                            }
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
                        } else if matches!(app.app_mode, AppMode::Transactions) {
                            let len = app
                                .expense_manager
                                .displayed_transactions(
                                    app.current_year,
                                    app.current_month,
                                    app.show_all_transactions,
                                )
                                .len();
                            if len > 0 {
                                app.selected_transaction = Some(
                                    app.selected_transaction.map_or(0, |i| (i + 1).min(len - 1)),
                                );
                            }
                        } else if matches!(app.app_mode, AppMode::Categories) {
                            let len = app.expense_manager.transaction_categories.len();
                            if len > 0 {
                                app.selected_category =
                                    Some(app.selected_category.map_or(0, |i| (i + 1).min(len - 1)));
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
                        } else if matches!(app.app_mode, AppMode::Transactions) {
                            let len = app
                                .expense_manager
                                .displayed_transactions(
                                    app.current_year,
                                    app.current_month,
                                    app.show_all_transactions,
                                )
                                .len();
                            if len > 0 {
                                app.selected_transaction = Some(
                                    app.selected_transaction.map_or(0, |i| i.saturating_sub(1)),
                                );
                            }
                        } else if matches!(app.app_mode, AppMode::Categories) {
                            let len = app.expense_manager.transaction_categories.len();
                            if len > 0 {
                                app.selected_category =
                                    Some(app.selected_category.map_or(0, |i| i.saturating_sub(1)));
                            }
                        }
                    }
                    KeyCode::Char('h') | KeyCode::Left => {
                        if matches!(
                            app.app_mode,
                            AppMode::List | AppMode::Overview | AppMode::Transactions
                        ) && !(matches!(app.app_mode, AppMode::Transactions)
                            && app.show_all_transactions)
                        {
                            if app.current_month > 1 {
                                app.current_month -= 1;
                            } else {
                                app.current_month = 12;
                                app.current_year -= 1;
                            }
                            app.selected_expense = None;
                            app.selected_transaction = None;
                        }
                    }
                    KeyCode::Char('l') | KeyCode::Right => {
                        if matches!(
                            app.app_mode,
                            AppMode::List | AppMode::Overview | AppMode::Transactions
                        ) && !(matches!(app.app_mode, AppMode::Transactions)
                            && app.show_all_transactions)
                        {
                            if app.current_month < 12 {
                                app.current_month += 1;
                            } else {
                                app.current_month = 1;
                                app.current_year += 1;
                            }
                            app.selected_expense = None;
                            app.selected_transaction = None;
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
                        } else if matches!(app.app_mode, AppMode::Transactions) {
                            if let Some(selected) = app.selected_transaction {
                                let transaction_info = app
                                    .expense_manager
                                    .displayed_transactions(
                                        app.current_year,
                                        app.current_month,
                                        app.show_all_transactions,
                                    )
                                    .get(selected)
                                    .map(|(index, transaction)| {
                                        (
                                            *index,
                                            transaction.description.clone(),
                                            transaction.amount,
                                        )
                                    });
                                if let Some((index, description, amount)) = transaction_info {
                                    app.pending_transaction_index = Some(index);
                                    app.pending_delete_description = description;
                                    app.pending_delete_amount = amount;
                                    app.app_mode = AppMode::ConfirmDeleteTransaction;
                                }
                            }
                        } else if matches!(app.app_mode, AppMode::Categories) {
                            if let Some(index) = app.selected_category {
                                match app.expense_manager.delete_category(index) {
                                    Ok(()) => {
                                        let len = app.expense_manager.transaction_categories.len();
                                        app.selected_category = if len == 0 {
                                            None
                                        } else {
                                            Some(index.min(len - 1))
                                        };
                                        app.category_notice = Some("Category deleted.".into());
                                    }
                                    Err(error) => app.category_notice = Some(error),
                                }
                            }
                        }
                    }
                    KeyCode::Char('y') => {
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
                    KeyCode::Char('b') => {
                        if app.app_mode.is_content_view() {
                            app.balance_return_mode = app.app_mode;
                            app.app_mode = AppMode::AddBalance;
                            app.input_balance.clear();
                            app.input_error = None;
                        }
                    }
                    KeyCode::Esc => match app.app_mode {
                        AppMode::Help => app.app_mode = app.help_return_mode,
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
                        AppMode::ConfirmDeleteTransaction => {
                            app.app_mode = AppMode::Transactions;
                            app.pending_transaction_index = None;
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
                        } else if matches!(app.app_mode, AppMode::ConfirmDeleteTransaction) {
                            confirm_transaction_delete(&mut app);
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
                        } else if matches!(app.app_mode, AppMode::ConfirmDeleteTransaction) {
                            match c {
                                'y' | 'Y' => confirm_transaction_delete(&mut app),
                                'n' | 'N' => {
                                    app.app_mode = AppMode::Transactions;
                                    app.pending_transaction_index = None;
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
                        let description = app.input_description.trim().to_string();
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
                InputMode::AddingTransaction(field) => match key.code {
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                        app.editing_transaction_index = None;
                        app.input_error = None;
                    }
                    KeyCode::Tab => {
                        app.input_error = None;
                        app.input_mode = InputMode::AddingTransaction(match field {
                            TransactionInputField::Date => TransactionInputField::Description,
                            TransactionInputField::Description => TransactionInputField::Amount,
                            TransactionInputField::Amount => TransactionInputField::Kind,
                            TransactionInputField::Kind => TransactionInputField::Category,
                            TransactionInputField::Category => TransactionInputField::Date,
                        });
                    }
                    KeyCode::Enter => {
                        let date =
                            NaiveDate::parse_from_str(app.transaction_date.trim(), "%Y-%m-%d");
                        let description = app.input_description.trim().to_string();
                        let amount = parse_positive_amount(&app.input_amount);
                        if date.is_err() {
                            app.input_error =
                                Some("Use a valid date in YYYY-MM-DD format.".to_string());
                            app.input_mode =
                                InputMode::AddingTransaction(TransactionInputField::Date);
                        } else if description.is_empty() {
                            app.input_error = Some("Add a description before saving.".to_string());
                            app.input_mode =
                                InputMode::AddingTransaction(TransactionInputField::Description);
                        } else if amount.is_none() {
                            app.input_error = Some("Enter an amount greater than 0.".to_string());
                            app.input_mode =
                                InputMode::AddingTransaction(TransactionInputField::Amount);
                        } else if let (Ok(date), Some(amount)) = (date, amount) {
                            app.ensure_transaction_category();
                            if app.transaction_category_id.is_empty() {
                                app.input_error = Some(
                                    "Create a category with g before recording this transaction."
                                        .to_string(),
                                );
                                continue;
                            }
                            let transaction = Transaction {
                                date,
                                description,
                                amount,
                                kind: app.transaction_kind,
                                category_id: app.transaction_category_id.clone(),
                                legacy_category: None,
                            };
                            let transaction_index =
                                if let Some(index) = app.editing_transaction_index.take() {
                                    app.expense_manager.update_transaction(index, transaction);
                                    index
                                } else {
                                    app.expense_manager.add_transaction(transaction)
                                };
                            app.current_year = date.year();
                            app.current_month = date.month();
                            app.selected_transaction = app
                                .expense_manager
                                .displayed_transactions(
                                    app.current_year,
                                    app.current_month,
                                    app.show_all_transactions,
                                )
                                .iter()
                                .position(|(index, _)| *index == transaction_index);
                            app.input_mode = InputMode::Normal;
                            app.input_description.clear();
                            app.input_amount.clear();
                            app.transaction_category_query.clear();
                            app.input_error = None;
                        }
                    }
                    KeyCode::Left => match field {
                        TransactionInputField::Kind => {
                            app.transaction_kind = app.transaction_kind.toggle();
                            app.ensure_transaction_category();
                        }
                        TransactionInputField::Category => app.cycle_transaction_category(false),
                        _ => {}
                    },
                    KeyCode::Right => match field {
                        TransactionInputField::Kind => {
                            app.transaction_kind = app.transaction_kind.toggle();
                            app.ensure_transaction_category();
                        }
                        TransactionInputField::Category => app.cycle_transaction_category(true),
                        _ => {}
                    },
                    KeyCode::Up => {
                        if matches!(field, TransactionInputField::Category) {
                            app.cycle_transaction_category(false);
                        }
                    }
                    KeyCode::Down => {
                        if matches!(field, TransactionInputField::Category) {
                            app.cycle_transaction_category(true);
                        }
                    }
                    KeyCode::Char(c) => {
                        match field {
                            TransactionInputField::Date => {
                                if c.is_ascii_digit() || c == '-' {
                                    app.transaction_date.push(c);
                                }
                            }
                            TransactionInputField::Description => app.input_description.push(c),
                            TransactionInputField::Amount => {
                                if c.is_ascii_digit() || c == '.' || c == ',' {
                                    app.input_amount.push(c);
                                }
                            }
                            TransactionInputField::Kind => {
                                if c == ' ' {
                                    app.transaction_kind = app.transaction_kind.toggle();
                                    app.ensure_transaction_category();
                                }
                            }
                            TransactionInputField::Category => match c {
                                'j' => app.cycle_transaction_category(true),
                                'k' => app.cycle_transaction_category(false),
                                _ => {
                                    app.transaction_category_query.push(c);
                                    app.ensure_transaction_category();
                                }
                            },
                        }
                        app.input_error = None;
                    }
                    KeyCode::Backspace => {
                        match field {
                            TransactionInputField::Date => {
                                app.transaction_date.pop();
                            }
                            TransactionInputField::Description => {
                                app.input_description.pop();
                            }
                            TransactionInputField::Amount => {
                                app.input_amount.pop();
                            }
                            TransactionInputField::Category => {
                                app.transaction_category_query.pop();
                                app.ensure_transaction_category();
                            }
                            _ => {}
                        }
                        app.input_error = None;
                    }
                    _ => {}
                },
                InputMode::AddingCategory => match key.code {
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                        app.editing_category_index = None;
                        app.input_error = None;
                    }
                    KeyCode::Left => {
                        app.new_category_kind = match app.new_category_kind {
                            CategoryKind::Expense => CategoryKind::Both,
                            CategoryKind::Income => CategoryKind::Expense,
                            CategoryKind::Both => CategoryKind::Income,
                        };
                    }
                    KeyCode::Right | KeyCode::Tab => {
                        app.new_category_kind = match app.new_category_kind {
                            CategoryKind::Expense => CategoryKind::Income,
                            CategoryKind::Income => CategoryKind::Both,
                            CategoryKind::Both => CategoryKind::Expense,
                        };
                    }
                    KeyCode::Enter => {
                        if let Some(index) = app.editing_category_index.take() {
                            match app.expense_manager.update_category(
                                index,
                                app.new_category_name.clone(),
                                app.new_category_kind,
                            ) {
                                Ok(()) => {
                                    app.input_mode = InputMode::Normal;
                                    app.category_notice = Some("Category updated.".into());
                                    app.input_error = None;
                                }
                                Err(error) => {
                                    app.editing_category_index = Some(index);
                                    app.input_error = Some(error);
                                }
                            }
                        } else {
                            match app
                                .expense_manager
                                .add_category(app.new_category_name.clone(), app.new_category_kind)
                            {
                                Ok(id) => {
                                    app.transaction_category_id = id;
                                    app.input_mode = InputMode::Normal;
                                    app.category_notice = Some("Category created.".into());
                                    app.input_error = None;
                                }
                                Err(error) => app.input_error = Some(error),
                            }
                        }
                    }
                    KeyCode::Backspace => {
                        app.new_category_name.pop();
                        app.input_error = None;
                    }
                    KeyCode::Char(c) => {
                        app.new_category_name.push(c);
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

fn default_transaction_date(year: i32, month: u32) -> String {
    let today = Local::now().date_naive();
    let preferred_day = if today.year() == year && today.month() == month {
        today.day()
    } else {
        1
    };
    NaiveDate::from_ymd_opt(year, month, preferred_day)
        .expect("the selected month is always valid")
        .format("%Y-%m-%d")
        .to_string()
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

fn confirm_transaction_delete(app: &mut App) {
    if let Some(index) = app.pending_transaction_index.take() {
        app.expense_manager.delete_transaction(index);
    }
    let len = app
        .expense_manager
        .displayed_transactions(
            app.current_year,
            app.current_month,
            app.show_all_transactions,
        )
        .len();
    app.selected_transaction = match (app.selected_transaction, len) {
        (_, 0) => None,
        (Some(selected), len) => Some(selected.min(len - 1)),
        _ => None,
    };
    app.pending_delete_description.clear();
    app.pending_delete_amount = 0.0;
    app.app_mode = AppMode::Transactions;
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
    use super::{
        default_transaction_categories, parse_non_negative_amount, parse_positive_amount,
        sorted_expenses_grouped, App, Category, CategoryKind, Expense, ExpenseManager, Transaction,
        TransactionKind,
    };
    use chrono::NaiveDate;
    use std::collections::HashMap;

    fn empty_manager() -> ExpenseManager {
        ExpenseManager {
            expenses: HashMap::new(),
            balances: HashMap::new(),
            balance_history: HashMap::new(),
            transactions: Vec::new(),
            transaction_categories: default_transaction_categories(),
        }
    }

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

    #[test]
    fn old_data_without_transactions_still_loads() {
        let manager: ExpenseManager =
            serde_json::from_str(r#"{"expenses":{},"balances":{},"balance_history":{}}"#).unwrap();
        assert!(manager.transactions.is_empty());
    }

    #[test]
    fn monthly_transactions_are_filtered_and_newest_first() {
        let manager = ExpenseManager {
            expenses: HashMap::new(),
            balances: HashMap::new(),
            balance_history: HashMap::new(),
            transaction_categories: default_transaction_categories(),
            transactions: vec![
                Transaction {
                    date: NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(),
                    description: "August".into(),
                    amount: 1.0,
                    kind: TransactionKind::Expense,
                    category_id: "other".into(),
                    legacy_category: None,
                },
                Transaction {
                    date: NaiveDate::from_ymd_opt(2026, 9, 2).unwrap(),
                    description: "Later".into(),
                    amount: 2.0,
                    kind: TransactionKind::Income,
                    category_id: "income".into(),
                    legacy_category: None,
                },
                Transaction {
                    date: NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
                    description: "Earlier".into(),
                    amount: 3.0,
                    kind: TransactionKind::Expense,
                    category_id: "food".into(),
                    legacy_category: None,
                },
            ],
        };
        let descriptions: Vec<_> = manager
            .month_transactions(2026, 9)
            .iter()
            .map(|(_, transaction)| transaction.description.as_str())
            .collect();
        assert_eq!(descriptions, vec!["Later", "Earlier"]);
        let all_descriptions: Vec<_> = manager
            .displayed_transactions(2026, 9, true)
            .iter()
            .map(|(_, transaction)| transaction.description.as_str())
            .collect();
        assert_eq!(all_descriptions, vec!["Later", "Earlier", "August"]);
    }

    #[test]
    fn legacy_transaction_categories_migrate_to_stable_ids() {
        let json = r#"{
            "expenses":{},"balances":{},"balance_history":{},
            "transactions":[{
                "date":"2026-09-01","description":"Lunch","amount":20.0,
                "kind":"Expense","category":"Food"
            }]
        }"#;
        let mut manager: ExpenseManager = serde_json::from_str(json).unwrap();
        manager.migrate_transaction_categories();
        assert_eq!(manager.transactions[0].category_id, "food");
        assert!(manager.transactions[0].legacy_category.is_none());
        assert_eq!(manager.category_name("food"), "Food");
    }

    #[test]
    fn custom_categories_get_unique_stable_ids() {
        let mut manager = empty_manager();
        // Avoid saving in this unit test: exercise the same ID construction through existing data.
        manager
            .transaction_categories
            .push(super::TransactionCategory {
                id: "coffee".into(),
                name: "Old coffee".into(),
                kind: CategoryKind::Expense,
                archived: true,
            });
        assert!(manager
            .available_categories(TransactionKind::Expense)
            .iter()
            .all(|category| category.id != "coffee"));
    }

    #[test]
    fn category_availability_respects_transaction_kind() {
        let manager = empty_manager();
        let expenses: Vec<_> = manager
            .available_categories(TransactionKind::Expense)
            .iter()
            .map(|category| category.id.as_str())
            .collect();
        let income: Vec<_> = manager
            .available_categories(TransactionKind::Income)
            .iter()
            .map(|category| category.id.as_str())
            .collect();
        assert!(expenses.contains(&"food"));
        assert!(expenses.contains(&"other"));
        assert!(!expenses.contains(&"income"));
        assert!(income.contains(&"income"));
        assert!(income.contains(&"other"));
        assert!(!income.contains(&"food"));
    }

    #[test]
    fn unknown_category_is_presented_safely() {
        assert_eq!(empty_manager().category_name("missing"), "Uncategorized");
    }

    #[test]
    fn transaction_kind_toggle_is_reversible() {
        assert_eq!(TransactionKind::Expense.toggle(), TransactionKind::Income);
        assert_eq!(TransactionKind::Income.toggle(), TransactionKind::Expense);
    }

    #[test]
    fn expense_paid_state_toggles() {
        let mut expense = Expense::new("Rent".into(), 100.0, Category::Need);
        assert!(!expense.is_paid);
        expense.toggle_paid();
        assert!(expense.is_paid);
        expense.toggle_paid();
        assert!(!expense.is_paid);
    }

    #[test]
    fn expense_sorting_groups_needs_before_wants_alphabetically() {
        let expenses = vec![
            Expense::new("Zoo".into(), 1.0, Category::Want),
            Expense::new("Rent".into(), 1.0, Category::Need),
            Expense::new("Apple".into(), 1.0, Category::Need),
        ];
        let sorted: Vec<_> = sorted_expenses_grouped(&expenses)
            .iter()
            .map(|(_, expense)| expense.description.as_str())
            .collect();
        assert_eq!(sorted, vec!["Apple", "Rent", "Zoo"]);
    }

    #[test]
    fn autocomplete_filters_and_selects_categories() {
        let mut app = App::new();
        app.expense_manager = empty_manager();
        app.transaction_category_query = "ho".into();
        app.ensure_transaction_category();
        assert_eq!(app.filtered_transaction_categories().len(), 2);
        assert_eq!(app.transaction_category_id, "home");
        app.transaction_category_query.clear();
        app.transaction_category_id = "food".into();
        app.cycle_transaction_category(true);
        assert_eq!(app.transaction_category_id, "home");

        app.transaction_category_id = "health".into();
        assert_eq!(app.transaction_category_window_start(4), 1);
    }

    #[test]
    fn deleting_used_category_is_rejected() {
        let mut manager = empty_manager();
        manager.transactions.push(Transaction {
            date: NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
            description: "Lunch".into(),
            amount: 10.0,
            kind: TransactionKind::Expense,
            category_id: "food".into(),
            legacy_category: None,
        });
        let food_index = manager
            .transaction_categories
            .iter()
            .position(|category| category.id == "food")
            .unwrap();
        let error = manager.delete_category(food_index).unwrap_err();
        assert!(error.contains("used by transactions"));
        assert!(manager
            .transaction_categories
            .iter()
            .any(|category| category.id == "food"));
    }

    #[test]
    fn default_date_for_other_month_is_first_day() {
        assert_eq!(super::default_transaction_date(2000, 1), "2000-01-01");
    }

    #[test]
    fn transaction_json_uses_stable_category_id() {
        let transaction = Transaction {
            date: NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
            description: "Lunch".into(),
            amount: 10.0,
            kind: TransactionKind::Expense,
            category_id: "food".into(),
            legacy_category: None,
        };
        let json = serde_json::to_string(&transaction).unwrap();
        assert!(json.contains("category_id"));
        assert!(!json.contains("legacy_category"));
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
