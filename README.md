# Jira to Obsidian Sync

A simple yet powerful CLI tool written in Rust to synchronize your Jira issues into an Obsidian Vault. It pulls tasks assigned to you, converts them into Markdown files, and generates a dynamic Kanban board compatible with the [Obsidian Kanban plugin](https://github.com/mgmeyers/obsidian-kanban).

## Features

- **One-Way Sync:** Fetches issues from Jira (JQL: `assignee = currentUser()`) and updates/creates Markdown files in your vault.
- **Rich Content:** Converts Jira ADF (Atlassian Document Format) descriptions into clean Markdown.
- **Metadata:** Adds Frontmatter (YAML) with status, priority, link, and sync date.
- **Safe Updates:** Preserves your personal notes in the Markdown file (everything under `%% GÜVENLİ BÖLGE %%`).
- **Kanban Board:** Automatically generates a `JiraKanban.md` file, grouping your tasks by their actual Jira status headers.

## Setup

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/your-username/jira-obsidian-sync.git
    cd jira-obsidian-sync
    ```

2.  **Configure Environment:**
    Create a `.env` file in the root directory with the following variables:

    ```env
    # Jira Cloud URL (e.g., your-company.atlassian.net)
    JIRA_SERVER=your-domain.atlassian.net

    # Your Jira Email
    JIRA_USER=your-email@example.com

    # Jira API Token (Create one at: https://id.atlassian.com/manage-profile/security/api-tokens)
    JIRA_API_TOKEN=your_api_token_here

    # Local path to your Obsidian Vault folder where files will be saved
    OBSIDIAN_PATH=/Users/username/Documents/ObsidianVault/Jira
    ```

## Usage

Run the tool using Cargo:

```bash
cargo run
```

Or build a release binary:

```bash
cargo build --release
./target/release/jira_obsidian_sync
```

The tool will:
1.  Connect to Jira.
2.  Fetch all issues assigned to you.
3.  Create/Update individual `.md` files for each task.
4.  Generate `JiraKanban.md` with your tasks organized by status.

---

Made with Rust 🦀
