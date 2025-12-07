# Recurring Costs Manager (TUI)

A terminal-based expense tracking application written in Rust, featuring a clean and intuitive Text User Interface (TUI). Keep track of your monthly expenses, monitor your balance, and manage recurring payments with ease.

![TUI Screenshot Placeholder]

## Features

### 💰 Financial Management
- Track monthly recurring expenses
- Set and monitor your available balance
- View free money after accounting for unpaid expenses
- Mark expenses as paid/unpaid
- Historical balance tracking

### 📊 Financial Overview
- Split view of available funds and monthly expenses
- Color-coded financial status indicators
- Payment progress tracking
- Monthly expense summaries
- Balance history log

### 🎯 User Experience
- Vim-style navigation (with arrow key support)
- Clean, intuitive interface
- Real-time updates
- Persistent data storage
- Monthly navigation

## 🚀 Quick Start

### Prerequisites
- Rust and Cargo installed on your system
- Terminal with Unicode support

### Installation

1. Clone the repository:
```bash
git clone [repository-url]
cd recurring-costs
```

2. Build the project:
```bash
cargo build --release
```

3. Run the application:
```bash
cargo run --release
```

## 🎮 Controls

### Main List View
- `j` or `↓`: Move selection down
- `k` or `↑`: Move selection up
- `h` or `←`: Previous month
- `l` or `→`: Next month
- `Space`: Toggle selected expense paid/unpaid
- `i`: Add new expense
- `o`: Switch to Overview mode
- `q`: Quit application

### Overview Mode
- `b`: Enter balance input mode
- `H`: View balance history
- `Esc`: Return to list mode

### Input Mode
- `Tab`: Switch between input fields
- `Enter`: Save input
- `Esc`: Cancel input

## 💾 Data Storage

The application automatically saves your data in:
- macOS: `~/Library/Application Support/recurring_costs/`
- Linux: `~/.local/share/recurring_costs/`
- Windows: `%APPDATA%/recurring_costs/`

## 📦 Dependencies

- `ratatui`: Terminal user interface library
- `crossterm`: Terminal manipulation
- `serde`: Serialization framework
- `chrono`: Date and time functionality

## 🤝 Contributing

Contributions are welcome! Feel free to:
- Report bugs
- Suggest new features
- Submit pull requests

## 📝 License

This project is licensed under the MIT License - see the LICENSE file for details.

## 🙏 Acknowledgments

- Built with [Ratatui](https://github.com/tui-rs-revival/ratatui)
- Inspired by various TUI applications in the Rust ecosystem
