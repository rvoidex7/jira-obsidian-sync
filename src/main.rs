use anyhow::{Context, Result};
use serde::Deserialize;
use std::env;

#[derive(Clone, Debug)]
struct Config {
    jira_host: String,
    jira_user: Option<String>,
    jira_token: String,
    obsidian_vault_path: String,
    jira_jql: String,
}

impl Config {
    fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();
        let jira_host = env::var("JIRA_HOST").context("JIRA_HOST must be set")?;
        let jira_token = env::var("JIRA_TOKEN").context("JIRA_TOKEN must be set")?;
        let obsidian_vault_path =
            env::var("OBSIDIAN_VAULT_PATH").context("OBSIDIAN_VAULT_PATH must be set")?;

        let jira_user = env::var("JIRA_USER").ok().filter(|s| !s.is_empty());
        let jira_jql = env::var("JIRA_JQL").unwrap_or_else(|_| {
            "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC".to_string()
        });

        Ok(Self {
            jira_host,
            jira_user,
            jira_token,
            obsidian_vault_path,
            jira_jql,
        })
    }
}

// Jira API Structs

#[derive(Debug, Deserialize)]
struct JiraSearchResponse {
    issues: Vec<JiraIssue>,
}

#[derive(Debug, Deserialize)]
struct JiraIssue {
    key: String,
    fields: JiraFields,
}

#[derive(Debug, Deserialize)]
struct JiraFields {
    summary: String,
    description: Option<JiraADF>,
    status: JiraStatus,
    created: String,
    priority: Option<JiraPriority>,
    #[serde(rename = "issuetype")]
    issue_type: JiraIssueType,
}

#[derive(Debug, Deserialize)]
struct JiraStatus {
    name: String,
}

#[derive(Debug, Deserialize)]
struct JiraPriority {
    name: String,
}

#[derive(Debug, Deserialize)]
struct JiraIssueType {
    name: String,
}

// Simplified ADF structure for parsing
#[derive(Debug, Deserialize)]
struct JiraADF {
    content: Option<Vec<JiraADFNode>>,
}

#[derive(Debug, Deserialize)]
struct JiraADFNode {
    #[serde(rename = "type")]
    node_type: String,
    content: Option<Vec<JiraADFNode>>,
    text: Option<String>,
}

fn extract_text_from_adf(adf: &JiraADF) -> String {
    let mut out = String::new();
    if let Some(content) = &adf.content {
        for node in content {
            extract_text_from_node(node, &mut out);
            out.push('\n');
        }
    }
    if out.trim().is_empty() {
        "No description provided.".to_string()
    } else {
        out.trim().to_string()
    }
}

fn extract_text_from_node(node: &JiraADFNode, out: &mut String) {
    if let Some(text) = &node.text {
        out.push_str(text);
    }
    if let Some(content) = &node.content {
        for child in content {
            extract_text_from_node(child, out);
        }
    }
    // Simple block handling
    match node.node_type.as_str() {
        "paragraph" => out.push_str("\n\n"),
        "bulletList" | "orderedList" => out.push('\n'),
        "listItem" => out.push_str("\n- "), // Simplified list handling
        _ => {}
    }
}

struct JiraClient {
    client: reqwest::Client,
    config: Config,
}

impl JiraClient {
    fn new(config: Config) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "Accept",
            "application/json".parse().unwrap(),
        );

        let client_builder = reqwest::Client::builder();

        // Auth Logic
        let client = if let Some(user) = &config.jira_user {
             // Basic Auth
             // reqwest::ClientBuilder does not have a basic_auth method,
             // but reqwest::RequestBuilder does. However, we want to set it globally?
             // No, ClientBuilder does not have basic_auth helper directly.
             // We have to use a default header or handle it per request.
             // OR, just set it on the builder? No.
             // Wait, I might be misremembering. Let's check docs or just use the header.

             // Correct way is to not set it on builder but on requests?
             // Actually, `default_headers` is fine, but `basic_auth` is not on ClientBuilder.
             // We can manually construct the Authorization header.

             use base64::{Engine as _, engine::general_purpose};
             let auth_val = format!("{}:{}", user, config.jira_token);
             let encoded = general_purpose::STANDARD.encode(auth_val);
             let mut header_value = reqwest::header::HeaderValue::from_str(&format!("Basic {}", encoded))
                .map_err(|e| anyhow::anyhow!("Invalid header value: {}", e))?;
             header_value.set_sensitive(true);
             headers.insert(reqwest::header::AUTHORIZATION, header_value);

             client_builder
                .default_headers(headers)
                .build()?
        } else {
             // PAT (Bearer)
             let mut auth_val = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", config.jira_token))?;
             auth_val.set_sensitive(true);
             headers.insert(reqwest::header::AUTHORIZATION, auth_val);

             client_builder
                .default_headers(headers)
                .build()?
        };

        Ok(Self { client, config })
    }

    async fn fetch_issues(&self) -> Result<Vec<JiraIssue>> {
        let url = format!("https://{}/rest/api/3/search", self.config.jira_host);
        let resp = self.client.get(&url)
            .query(&[
                ("jql", &self.config.jira_jql),
                ("fields", &"key,summary,description,status,created,priority,issuetype".to_string())
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            let error_text = resp.text().await?;
            anyhow::bail!("Jira API Error: {}", error_text);
        }

        let search_results: JiraSearchResponse = resp.json().await?;
        Ok(search_results.issues)
    }
}

async fn generate_kanban_board(config: &Config, issues: &[JiraIssue]) -> Result<()> {
    use std::collections::HashMap;
    use tokio::fs;

    let mut issues_by_status: HashMap<String, Vec<&JiraIssue>> = HashMap::new();

    // Group issues by status name
    for issue in issues {
        issues_by_status.entry(issue.fields.status.name.clone())
            .or_default()
            .push(issue);
    }

    let mut board_content = String::from("---\nkanban-plugin: basic\n---\n\n");

    // Sort keys for deterministic output or just iterate.
    // Usually one wants a specific order, but requirement says "Dynamically map".
    // We can iterate the map. To keep it somewhat stable, we could sort by status name,
    // but often "To Do" < "In Progress" < "Done". Since we don't know the workflow,
    // we will just sort alphabetically or by occurrence order if we had that.
    // Let's sort alphabetically for stability.
    let mut statuses: Vec<_> = issues_by_status.keys().cloned().collect();
    statuses.sort();

    for status in statuses {
        board_content.push_str(&format!("## {}\n\n", status));

        if let Some(issues_in_status) = issues_by_status.get(&status) {
            for issue in issues_in_status {
                // Kanbn plugin format: - [ ] [[File Link]]
                // Or just [[File Link]] works too usually, but "checklist" style is standard for tasks.
                // Requirement example:
                // To Do
                //   [ ]]
                //
                // Wait, the example was:
                // To Do
                //    [ ]]
                //
                // The example text in prompt:
                // kanban-plugin: basic
                // To Do
                //
                //    [ ]]

                // Assuming standard Obsidian Kanban syntax which is usually a list of tasks.
                // "- [ ] [[Link]]" is the most robust way.

                board_content.push_str(&format!("- [ ] [[Jira Tickets/{}]]\n", issue.key));
            }
        }
        board_content.push('\n');
    }

    let board_path = std::path::Path::new(&config.obsidian_vault_path).join("My Jira Board.md");
    fs::write(board_path, board_content).await?;
    println!("Generated Kanban Board");

    Ok(())
}

async fn update_issue_file(config: &Config, issue: &JiraIssue) -> Result<()> {
    use tokio::fs;

    use std::path::Path;

    let safe_area_delimiter = "%% USER_NOTES_START %%";
    let tickets_dir = Path::new(&config.obsidian_vault_path).join("Jira Tickets");

    if !tickets_dir.exists() {
        fs::create_dir_all(&tickets_dir).await?;
    }

    let file_path = tickets_dir.join(format!("{}.md", issue.key));
    let mut user_notes = String::from("\n- [ ] ");

    if file_path.exists() {
        let content = fs::read_to_string(&file_path).await?;
        if let Some((_, notes)) = content.split_once(safe_area_delimiter) {
            user_notes = notes.to_string();
        }
    }

    let description = if let Some(adf) = &issue.fields.description {
        extract_text_from_adf(adf)
    } else {
        "No description.".to_string()
    };

    let jira_url = format!("https://{}/browse/{}", config.jira_host, issue.key);

    let frontmatter = format!(
        "---\njira_key: {}\njira_status: \"{}\"\njira_url: {}\ncreated_at: {}\n---\n",
        issue.key,
        issue.fields.status.name,
        jira_url,
        issue.fields.created
    );

    let body = format!(
        "# {} {}\n\n**Type**: {}\n**Priority**: {}\n**Status**: {}\n\n## Description\n{}\n\n{}\n{}",
        issue.key,
        issue.fields.summary,
        issue.fields.issue_type.name,
        issue.fields.priority.as_ref().map(|p| p.name.as_str()).unwrap_or("None"),
        issue.fields.status.name,
        description,
        safe_area_delimiter,
        user_notes
    );

    let full_content = format!("{}{}", frontmatter, body);
    fs::write(file_path, full_content).await?;
    println!("Synced {}", issue.key);

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;
    println!("Starting sync for Jira Host: {}", config.jira_host);

    let client = JiraClient::new(config.clone())?;
    println!("Fetching issues...");
    let issues = client.fetch_issues().await?;
    println!("Found {} issues.", issues.len());

    if issues.is_empty() {
        println!("No issues found. Exiting.");
        return Ok(());
    }

    for issue in &issues {
        update_issue_file(&config, issue).await?;
    }

    generate_kanban_board(&config, &issues).await?;

    println!("Sync complete!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_from_adf() {
        let json_data = r#"
        {
            "version": 1,
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    "content": [
                        {
                            "type": "text",
                            "text": "Hello "
                        },
                        {
                            "type": "text",
                            "text": "World"
                        }
                    ]
                }
            ]
        }
        "#;
        let adf: JiraADF = serde_json::from_str(json_data).unwrap();
        let text = extract_text_from_adf(&adf);
        assert_eq!(text, "Hello World");
    }

    #[test]
    fn test_extract_text_empty() {
        let json_data = r#"
        {
            "version": 1,
            "type": "doc",
            "content": []
        }
        "#;
        let adf: JiraADF = serde_json::from_str(json_data).unwrap();
        let text = extract_text_from_adf(&adf);
        assert_eq!(text, "No description provided.");
    }
}
