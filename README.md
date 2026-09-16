# Recurring Costs

A fast, keyboard-first expense tracker for the terminal. Recurring Costs keeps each month’s bills, available balance, spending mix, and payment progress together in a focused Ratatui interface.

![Recurring Costs main view](docs/screenshot.svg)

## Highlights

- Organize expenses by month and by **Need** or **Want**.
- Mark bills as paid without removing them from the monthly plan.
- See balance, free money, category split, and payment progress at a glance.
- Copy a recurring expense into the following month.
- Keep a history of balance updates.
- Optionally keep a separate, dated transaction ledger for actual cash activity.
- Record both spending and income across practical or user-created categories.
- Review monthly spending totals grouped by category.
- Compare recorded spending with the monthly plan without changing the plan.
- Use Vim keys or arrow keys throughout the interface.
- Store data locally—no account, server, or network connection required.

## Getting started

You’ll need a recent Rust toolchain and a terminal with Unicode support.

```sh
git clone <repository-url>
cd expense-tracker
cargo run --release
```

To build a standalone binary instead:

```sh
cargo build --release
./target/release/recurring_costs
```

## Controls

Press `?` from any screen to open the complete keyboard shortcut reference. Press `Esc` to return.

### Expense list

| Key | Action |
| --- | --- |
| `j` / `↓`, `k` / `↑` | Select an expense |
| `h` / `←`, `l` / `→` | Move between months |
| `Space` | Mark the selected expense paid or unpaid |
| `c` | Create an expense |
| `t` | Switch the selected expense between Need and Want |
| `y` | Copy the selected expense to the next month |
| `d` | Delete the selected expense |
| `b` | Update the month’s balance |
| `o` | Open the full overview |
| `Shift+H` / `Shift+L` | Move between Plan, Transactions, and Categories |
| `q` | Quit |

The top navigation labels can also be clicked with the mouse.

### Transactions

Transaction tracking is an optional, separate workflow. Nothing recorded in the ledger changes the monthly plan or marks a planned expense as paid.

| Key | Action |
| --- | --- |
| `c` | Create a transaction |
| `e` | Edit the selected transaction |
| `v` | Toggle between the selected month and all transactions |
| `j` / `↓`, `k` / `↑` | Select a transaction |
| `h` / `←`, `l` / `→` | Move between months |
| `d` | Delete the selected transaction |
| `Shift+H` / `Shift+L` | Move between Plan, Transactions, and Categories |
| `q` | Quit |

The transaction form records a date, description, positive amount, type (expense or income), and category. Type into the category field to filter it, then use `j/k` or `↑/↓` to select a match. Use `Tab` between fields, `Enter` to save, and `Esc` to cancel. New categories can apply to expenses, income, or both.

### Categories

The Categories screen lists every category, whether it applies to expenses or income, its usage count, and its status. Use `j` / `k` to select, `c` to create, `e` to edit, and `d` to delete. Categories used by transactions cannot be deleted.

### Overview and history

| Key | Action |
| --- | --- |
| `b` | Update the balance |
| `H` | Open balance history |
| `Esc` | Go back |
| `q` | Quit |

### Dialogs

Use `Tab` to move between expense fields, the arrow keys or `n` / `w` to choose a category, `Enter` to save, and `Esc` to cancel. Amounts accept either a decimal point or comma.

## Local data

The application saves `expenses_data.json` and its log inside the platform data directory:

- macOS: `~/Library/Application Support/recurring_costs/`
- Linux: `~/.local/share/recurring_costs/`
- Windows: `%APPDATA%/recurring_costs/`

Back up `expenses_data.json` if you want to move your data to another machine.

## Built with

- [Ratatui](https://ratatui.rs/) for the terminal UI
- [Crossterm](https://github.com/crossterm-rs/crossterm) for terminal input and output
- [Serde](https://serde.rs/) for local data serialization
- [Chrono](https://github.com/chronotope/chrono) for dates and balance history

## Contributing

Bug reports, feature ideas, and pull requests are welcome. Before opening a pull request, run:

```sh
cargo fmt -- --check
cargo test
```

## License

Licensed under the [MIT License](LICENSE).
