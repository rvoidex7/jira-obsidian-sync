use std::env;
use std::fs;
use std::path::Path;
use std::error::Error;
use std::io::Write;
use std::collections::HashMap;
use reqwest::Client;
use serde::Deserialize;
use regex::Regex;
use chrono::Local;
use dotenv::dotenv;

#[derive(Debug, Deserialize)]
struct JiraSearchResponse {
    issues: Vec<Issue>,
}

#[derive(Debug, Deserialize)]
struct Issue {
    key: String,
    fields: Fields,
}

#[derive(Debug, Deserialize)]
struct Fields {
    summary: String,
    description: Option<serde_json::Value>,
    status: Status,
    priority: Option<Priority>,
}

#[derive(Debug, Deserialize)]
struct Status {
    name: String,
    #[serde(rename = "statusCategory")]
    status_category: Option<StatusCategory>,
}

#[derive(Debug, Deserialize)]
struct StatusCategory {
    key: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct Priority {
    name: String,
}

const JQL_QUERY: &str = "assignee = currentUser() ORDER BY updated DESC";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok(); //.env dosyasını yükle

    // Ayarları oku
    let jira_server = env::var("JIRA_SERVER").expect(".env dosyasında JIRA_SERVER eksik");
    let jira_user = env::var("JIRA_USER").unwrap_or_default();
    let jira_token = env::var("JIRA_API_TOKEN").expect(".env dosyasında JIRA_API_TOKEN eksik");
    let vault_path = env::var("OBSIDIAN_PATH").expect(".env dosyasında OBSIDIAN_PATH eksik");

    println!("🚀 Jira Özel Hattı Başlatılıyor...");

    let client = Client::new();
    let search_url = format!("https://{}/rest/api/3/search/jql", jira_server);

    // Create a JSON body for the POST request
    let request_body = serde_json::json!({
        "jql": JQL_QUERY,
        "fields": [
            "key",
            "summary",
            "description",
            "status",
            "priority"
        ]
    });

    // İstek ayarları (Cloud veya Server ayrımı)
    let request_builder = client.post(&search_url)
       .json(&request_body);

    let request = if!jira_user.is_empty() {
        request_builder.basic_auth(jira_user, Some(jira_token))
    } else {
        request_builder.bearer_auth(jira_token)
    };

    let resp = request.send().await?;

    if!resp.status().is_success() {
        println!("❌ Hata: Jira bağlantısı başarısız oldu. Kod: {}", resp.status());
        let body = resp.text().await?;
        println!("Detay: {}", body);
        return Ok(());
    }

    let search_results: JiraSearchResponse = resp.json().await?;
    println!("🔍 {} adet aktif iş bulundu. İşleniyor...", search_results.issues.len());

    // Klasörü oluştur (yoksa)
    fs::create_dir_all(&vault_path)?;

    for issue in &search_results.issues {
        process_issue(issue, &jira_server, &vault_path)?;
    }

    let kanban_content = create_kanban_markdown(&search_results.issues);
    let kanban_path = Path::new(&vault_path).join("JiraKanban.md");
    let mut file = fs::File::create(&kanban_path)?;
    write!(file, "{}", kanban_content)?;
    println!("📋 Kanban panosu güncellendi: JiraKanban.md");

    println!("🏁 Senkronizasyon tamamlandı.");
    Ok(())
}

fn create_kanban_markdown(issues: &Vec<Issue>) -> String {
    let mut board: HashMap<String, Vec<&Issue>> = HashMap::new();
    let mut status_order: HashMap<String, i32> = HashMap::new();

    for issue in issues {
        let status_name = &issue.fields.status.name;
        board.entry(status_name.clone()).or_default().push(issue);

        if !status_order.contains_key(status_name) {
            let order = if let Some(cat) = &issue.fields.status.status_category {
                match cat.key.as_str() {
                    "new" => 0,
                    "indeterminate" => 1,
                    "done" => 2,
                    _ => 3,
                }
            } else {
                3
            };
            status_order.insert(status_name.clone(), order);
        }
    }

    let mut sorted_statuses: Vec<String> = board.keys().cloned().collect();
    sorted_statuses.sort_by(|a, b| {
        let order_a = status_order.get(a).unwrap_or(&3);
        let order_b = status_order.get(b).unwrap_or(&3);
        if order_a == order_b {
            a.cmp(b)
        } else {
            order_a.cmp(order_b)
        }
    });

    let mut markdown = String::from("---\nkanban-plugin: board\n---\n");

    for status in &sorted_statuses {
        markdown.push_str(&format!("\n## {}\n\n", status));
        if let Some(issues) = board.get(status) {
            for issue in issues {
                markdown.push_str(&format!("- [ ] [[{}]]\n", issue.key));
            }
        }
    }

    let col_count = sorted_statuses.len();
    let collapse_list: Vec<bool> = vec![false; col_count];
    let collapse_json = serde_json::to_string(&collapse_list).unwrap_or_else(|_| "[]".to_string());

    markdown.push_str("\n\n%% kanban:settings\n```\n");
    markdown.push_str(&format!(r#"{{"kanban-plugin":"board","list-collapse":{}}}"#, collapse_json));
    markdown.push_str("\n```\n%%\n");

    markdown
}

fn process_issue(issue: &Issue, server: &str, path: &str) -> Result<(), Box<dyn Error>> {
    let file_name = format!("{}.md", issue.key);
    let file_path = Path::new(path).join(file_name);

    let priority_name = issue.fields.priority.as_ref().map(|p| p.name.as_str()).unwrap_or("Belirsiz");
    let description = issue.fields.description.as_ref().map_or(String::new(), |d| extract_text_from_doc(d));
    let markdown_desc = jira_to_markdown(&description);
    let link = format!("https://{}/browse/{}", server, issue.key);
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let frontmatter = format!(r#"---
jira_key: {key}
jira_status: {status}
jira_priority: {priority}
jira_link: {link}
updated: {date}
tags: [jira, task]
---
# {key}: {summary}

> [!INFO] Jira Detayları
> **Durum:** `{status}` | **Öncelik:** `{priority}`
> **Link:** [Jira'da Aç]({link})
> **Son Sync:** {date}

## 📄 Açıklama
{desc}

---
%% GÜVENLİ BÖLGE: Bu satırın altındakiler silinmez %%
"#,
        key=issue.key,
        status=issue.fields.status.name,
        priority=priority_name,
        link=link,
        date=now,
        summary=issue.fields.summary,
        desc=markdown_desc
    );

    let mut personal_notes = String::from("\n## 🧠 Kişisel Notlarım\n- [ ] Buraya not alabilirsin.\n");

    if file_path.exists() {
        let content = fs::read_to_string(&file_path)?;
        let splitter = "%% GÜVENLİ BÖLGE: Bu satırın altındakiler silinmez %%";
        if let Some(parts) = content.split_once(splitter) {
            personal_notes = parts.1.to_string();
        }
    }

    let mut file = fs::File::create(&file_path)?;
    write!(file, "{}{}", frontmatter, personal_notes)?;

    println!("✅ Yazıldı: {}", issue.key);
    Ok(())
}

fn extract_text_from_doc(doc: &serde_json::Value) -> String {
    let mut text = String::new();
    if let Some(content) = doc.get("content").and_then(|c| c.as_array()) {
        for item in content {
            if let Some(content) = item.get("content").and_then(|c| c.as_array()) {
                for sub_item in content {
                    if let Some(text_content) = sub_item.get("text").and_then(|t| t.as_str()) {
                        text.push_str(text_content);
                    }
                }
            }
            // Add a newline after each top-level content item to preserve some structure
            text.push('\n');
        }
    }
    text
}

fn jira_to_markdown(text: &str) -> String {
    if text.is_empty() { return String::new(); }
    let mut t = text.to_string();

    // Basit Regex Değişimleri
    t = Regex::new(r"(?m)^h1\.").unwrap().replace_all(&t, "#").to_string();
    t = Regex::new(r"(?m)^h2\.").unwrap().replace_all(&t, "##").to_string();
    t = Regex::new(r"(?m)^h3\.").unwrap().replace_all(&t, "###").to_string();
    t = Regex::new(r"\*([^*\r\n]+)\*").unwrap().replace_all(&t, "**$1**").to_string();
    t = Regex::new(r"\{code(:([a-z]+))?\}").unwrap().replace_all(&t, "```$2").to_string();
    t = Regex::new(r"\{noformat\}").unwrap().replace_all(&t, "```").to_string();
    t = Regex::new(r"\[([^|\]]+)\|([^\]]+)\]").unwrap().replace_all(&t, "[$1]($2)").to_string();

    t
}

#[cfg(test)]
#[test]
fn test_kanban_logic() {
    let issue1 = Issue {
        key: "TASK-1".to_string(),
        fields: Fields {
            summary: "Task 1".to_string(),
            description: None,
            status: Status {
                name: "To Do".to_string(),
                status_category: Some(StatusCategory {
                    key: "new".to_string(),
                    name: "To Do".to_string(),
                }),
            },
            priority: None,
        },
    };

    let issue2 = Issue {
        key: "TASK-2".to_string(),
        fields: Fields {
            summary: "Task 2".to_string(),
            description: None,
            status: Status {
                name: "Done".to_string(),
                status_category: Some(StatusCategory {
                    key: "done".to_string(),
                    name: "Done".to_string(),
                }),
            },
            priority: None,
        },
    };

    let issues = vec![issue1, issue2];
    let markdown = create_kanban_markdown(&issues);

    assert!(markdown.contains("## To Do"));
    assert!(markdown.contains("## Done"));
    assert!(markdown.contains("- [ ] [[TASK-1]]"));
    assert!(markdown.contains("- [ ] [[TASK-2]]"));

    // Check order: To Do (new) comes before Done (done)
    let todo_pos = markdown.find("## To Do").unwrap();
    let done_pos = markdown.find("## Done").unwrap();
    assert!(todo_pos < done_pos);

    // Check settings
    assert!(markdown.contains(r#"{"kanban-plugin":"board","list-collapse":[false,false]}"#));
}
