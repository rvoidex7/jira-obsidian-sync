use std::env;
use std::fs;
use std::path::Path;
use std::error::Error;
use std::io::Write;
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
}

#[derive(Debug, Deserialize)]
struct Priority {
    name: String,
}

const JQL_QUERY: &str = "assignee = currentUser() AND statusCategory!= Done ORDER BY updated DESC";

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

    for issue in search_results.issues {
        process_issue(&issue, &jira_server, &vault_path)?;
    }

    println!("🏁 Senkronizasyon tamamlandı.");
    Ok(())
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
