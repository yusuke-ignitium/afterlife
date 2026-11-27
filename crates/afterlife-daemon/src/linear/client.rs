use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const LINEAR_AUTHORIZE_URL: &str = "https://linear.app/oauth/authorize";
pub const LINEAR_TOKEN_URL: &str = "https://api.linear.app/oauth/token";
pub const LINEAR_GRAPHQL_URL: &str = "https://api.linear.app/graphql";
pub const LINEAR_OAUTH_SCOPES: &str = "read,comments:create";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinearApiError {
    Unauthorized(String),
    InvalidGrant(String),
    Transport(String),
    GraphQl(String),
}

impl std::fmt::Display for LinearApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized(msg) => write!(f, "linear unauthorized: {msg}"),
            Self::InvalidGrant(msg) => write!(f, "linear invalid_grant: {msg}"),
            Self::Transport(msg) => write!(f, "linear transport error: {msg}"),
            Self::GraphQl(msg) => write!(f, "linear graphql error: {msg}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearTokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearViewer {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearComment {
    pub id: String,
    pub body: String,
    pub url: String,
    pub created_at: String,
    pub author_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinearIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub url: String,
    pub state_name: String,
    pub state_type: String,
    pub comments: Vec<LinearComment>,
}

pub trait LinearApi: Send + Sync {
    fn exchange_code(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        code: &str,
        redirect_uri: &str,
        code_verifier: &str,
    ) -> Result<LinearTokenResponse, LinearApiError>;

    fn refresh_token(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        refresh_token: &str,
    ) -> Result<LinearTokenResponse, LinearApiError>;

    fn fetch_viewer(&self, access_token: &str) -> Result<LinearViewer, LinearApiError>;

    fn fetch_started_assigned_issues(
        &self,
        access_token: &str,
    ) -> Result<(LinearViewer, Vec<LinearIssue>), LinearApiError>;

    fn create_comment(
        &self,
        access_token: &str,
        issue_id: &str,
        body: &str,
    ) -> Result<LinearComment, LinearApiError>;
}

pub struct HttpLinearClient {
    agent: ureq::Agent,
}

impl Default for HttpLinearClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpLinearClient {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(20))
            .timeout_write(Duration::from_secs(10))
            .build();
        Self { agent }
    }

    fn post_token_form(&self, params: &[(&str, &str)]) -> Result<LinearTokenResponse, LinearApiError> {
        let resp = self.agent.post(LINEAR_TOKEN_URL).send_form(params);
        match resp {
            Ok(ok) => {
                let raw = ok
                    .into_string()
                    .map_err(|e| LinearApiError::Transport(e.to_string()))?;
                serde_json::from_str::<LinearTokenResponse>(&raw)
                    .map_err(|e| LinearApiError::Transport(format!("invalid token response: {e}")))
            }
            Err(ureq::Error::Status(code, response)) => {
                let body = response.into_string().unwrap_or_default();
                if body.contains("invalid_grant") {
                    return Err(LinearApiError::InvalidGrant(format!("HTTP {code}: {body}")));
                }
                if code == 401 {
                    return Err(LinearApiError::Unauthorized(format!("HTTP 401: {body}")));
                }
                Err(LinearApiError::Transport(format!("HTTP {code}: {body}")))
            }
            Err(ureq::Error::Transport(t)) => Err(LinearApiError::Transport(t.to_string())),
        }
    }

    fn post_graphql(
        &self,
        access_token: &str,
        query: &str,
        variables: Value,
    ) -> Result<Value, LinearApiError> {
        let payload = json!({
            "query": query,
            "variables": variables,
        });
        let raw_payload = serde_json::to_string(&payload)
            .map_err(|e| LinearApiError::Transport(format!("serialize graphql payload: {e}")))?;
        let resp = self
            .agent
            .post(LINEAR_GRAPHQL_URL)
            .set("Authorization", &format!("Bearer {access_token}"))
            .set("Content-Type", "application/json")
            .send_string(&raw_payload);
        match resp {
            Ok(ok) => {
                let raw_body = ok
                    .into_string()
                    .map_err(|e| LinearApiError::Transport(format!("read graphql body: {e}")))?;
                let val: Value = serde_json::from_str(&raw_body)
                    .map_err(|e| LinearApiError::Transport(format!("invalid graphql JSON: {e}")))?;
                if let Some(errors) = val.get("errors").and_then(Value::as_array)
                    && !errors.is_empty()
                {
                    let msg = errors
                        .iter()
                        .filter_map(|e| e.get("message").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("; ");
                    if msg.to_ascii_lowercase().contains("authentication")
                        || msg.to_ascii_lowercase().contains("unauthorized")
                    {
                        return Err(LinearApiError::Unauthorized(msg));
                    }
                    return Err(LinearApiError::GraphQl(if msg.is_empty() {
                        errors[0].to_string()
                    } else {
                        msg
                    }));
                }
                val.get("data")
                    .cloned()
                    .ok_or_else(|| LinearApiError::GraphQl("missing data field".into()))
            }
            Err(ureq::Error::Status(401, response)) => {
                let body = response.into_string().unwrap_or_default();
                Err(LinearApiError::Unauthorized(format!("HTTP 401: {body}")))
            }
            Err(ureq::Error::Status(code, response)) => {
                let body = response.into_string().unwrap_or_default();
                Err(LinearApiError::Transport(format!("HTTP {code}: {body}")))
            }
            Err(ureq::Error::Transport(t)) => Err(LinearApiError::Transport(t.to_string())),
        }
    }
}

const VIEWER_QUERY: &str = "
query AfterlifeViewer {
  viewer {
    id
    name
    email
  }
}
";

const STARTED_ASSIGNED_ISSUES_QUERY: &str = "
query AfterlifeStartedAssignedIssues {
  viewer {
    id
    name
    email
    assignedIssues(
      first: 50
      filter: { state: { type: { eq: \"started\" } } }
    ) {
      nodes {
        id
        identifier
        title
        description
        url
        state {
          name
          type
        }
        comments(first: 100) {
          nodes {
            id
            body
            url
            createdAt
            user {
              name
            }
          }
        }
      }
    }
  }
}
";

const COMMENT_CREATE_MUTATION: &str = "
mutation AfterlifeCommentCreate($issueId: String!, $body: String!) {
  commentCreate(input: { issueId: $issueId, body: $body }) {
    success
    comment {
      id
      body
      url
      createdAt
      user {
        name
      }
    }
  }
}
";

fn parse_viewer(v: &Value) -> Result<LinearViewer, LinearApiError> {
    let id = v
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| LinearApiError::GraphQl("viewer.id missing".into()))?
        .to_string();
    let name = v
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Linear User")
        .to_string();
    let email = v
        .get("email")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    Ok(LinearViewer { id, name, email })
}

fn parse_comment(v: &Value, fallback_url: &str) -> Option<LinearComment> {
    let id = v.get("id").and_then(Value::as_str)?.to_string();
    let body = v.get("body").and_then(Value::as_str).unwrap_or("").to_string();
    let url = v
        .get("url")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(fallback_url)
        .to_string();
    let created_at = v
        .get("createdAt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let author_name = v
        .get("user")
        .and_then(|u| u.get("name"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    Some(LinearComment {
        id,
        body,
        url,
        created_at,
        author_name,
    })
}

impl LinearApi for HttpLinearClient {
    fn exchange_code(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        code: &str,
        redirect_uri: &str,
        code_verifier: &str,
    ) -> Result<LinearTokenResponse, LinearApiError> {
        let mut params = vec![
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", code_verifier),
        ];
        if let Some(secret) = client_secret.filter(|s| !s.trim().is_empty()) {
            params.push(("client_secret", secret));
        }
        self.post_token_form(&params)
    }

    fn refresh_token(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        refresh_token: &str,
    ) -> Result<LinearTokenResponse, LinearApiError> {
        let mut params = vec![
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token),
        ];
        if let Some(secret) = client_secret.filter(|s| !s.trim().is_empty()) {
            params.push(("client_secret", secret));
        }
        self.post_token_form(&params)
    }

    fn fetch_viewer(&self, access_token: &str) -> Result<LinearViewer, LinearApiError> {
        let data = self.post_graphql(access_token, VIEWER_QUERY, json!({}))?;
        let viewer = data
            .get("viewer")
            .ok_or_else(|| LinearApiError::GraphQl("viewer missing".into()))?;
        parse_viewer(viewer)
    }

    fn fetch_started_assigned_issues(
        &self,
        access_token: &str,
    ) -> Result<(LinearViewer, Vec<LinearIssue>), LinearApiError> {
        let data = self.post_graphql(access_token, STARTED_ASSIGNED_ISSUES_QUERY, json!({}))?;
        let viewer_val = data
            .get("viewer")
            .ok_or_else(|| LinearApiError::GraphQl("viewer missing".into()))?;
        let viewer = parse_viewer(viewer_val)?;
        let nodes = viewer_val
            .get("assignedIssues")
            .and_then(|a| a.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut issues = Vec::with_capacity(nodes.len());
        for node in nodes {
            let Some(id) = node.get("id").and_then(Value::as_str) else {
                continue;
            };
            let identifier = node
                .get("identifier")
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_string();
            let title = node
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let description = node
                .get("description")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            let url = node
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let state_name = node
                .get("state")
                .and_then(|s| s.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("Started")
                .to_string();
            let state_type = node
                .get("state")
                .and_then(|s| s.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("started")
                .to_string();
            let mut comments: Vec<LinearComment> = node
                .get("comments")
                .and_then(|c| c.get("nodes"))
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| parse_comment(c, &url))
                        .collect()
                })
                .unwrap_or_default();
            comments.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
            issues.push(LinearIssue {
                id: id.to_string(),
                identifier,
                title,
                description,
                url,
                state_name,
                state_type,
                comments,
            });
        }
        Ok((viewer, issues))
    }

    fn create_comment(
        &self,
        access_token: &str,
        issue_id: &str,
        body: &str,
    ) -> Result<LinearComment, LinearApiError> {
        let data = self.post_graphql(
            access_token,
            COMMENT_CREATE_MUTATION,
            json!({
                "issueId": issue_id,
                "body": body,
            }),
        )?;
        let created = data
            .get("commentCreate")
            .ok_or_else(|| LinearApiError::GraphQl("commentCreate missing".into()))?;
        let success = created
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !success {
            return Err(LinearApiError::GraphQl("commentCreate returned success=false".into()));
        }
        let comment = created
            .get("comment")
            .and_then(|c| parse_comment(c, ""))
            .ok_or_else(|| LinearApiError::GraphQl("commentCreate.comment missing".into()))?;
        Ok(comment)
    }
}
