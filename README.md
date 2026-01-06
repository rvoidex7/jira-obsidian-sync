# Jira to Obsidian Sync Tool

A Rust-based CLI tool to sync Jira issues to an Obsidian Vault, optimized for the "Obsidian Kanban" plugin.

## Features

- **One-way Sync**: Fetches issues from Jira and updates Obsidian.
- **Kanban Support**: Automatically generates a `My Jira Board.md` file compatible with the Obsidian Kanban plugin.
- **Safe Area**: Preserves user notes in issue files (everything below `%% USER_NOTES_START %%`).
- **Zero Config Status Mapping**: Dynamically creates Kanban columns based on Jira statuses.
- **ADF Parsing**: Basic extraction of text from Jira's Atlassian Document Format.

## Setup

1.  **Prerequisites**:
    *   Rust (cargo) installed.
    *   A Jira Cloud account.
    *   An Obsidian Vault.

2.  **Configuration**:
    Create a `.env` file in the project root with the following variables:

    ```env
    # Jira Host (e.g., your-domain.atlassian.net)
    JIRA_HOST=your-domain.atlassian.net

    # Jira User (Email). Required for Basic Auth.
    # If using PAT, leave this empty or remove it.
    JIRA_USER=your-email@example.com

    # Jira Token (API Token for Basic Auth OR PAT)
    JIRA_TOKEN=your-api-token-or-pat

    # Path to your Obsidian Vault (Absolute path recommended)
    OBSIDIAN_VAULT_PATH=/path/to/your/obsidian/vault

    # Optional: Custom JQL (Defaults to fetching open issues assigned to current user)
    # JIRA_JQL=assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC
    ```

## Usage

Run the tool using cargo:

```bash
cargo run
```

Or build a release binary:

```bash
cargo build --release
./target/release/jira-obsidian-sync
```

## How it works

1.  **Connects to Jira**: Authenticates using the credentials in `.env`.
2.  **Fetches Issues**: Runs the JQL query to get relevant issues.
3.  **Updates Tickets**:
    *   Creates a `Jira Tickets` folder in your vault.
    *   Creates/Updates `PROJ-123.md` files.
    *   Preserves any text below `%% USER_NOTES_START %%` in existing files.
4.  **Updates Board**:
    *   Overwrites `My Jira Board.md` in the vault root.
    *   Groups issues by status (e.g., `## To Do`, `## In Progress`).
