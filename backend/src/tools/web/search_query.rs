use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolError, ToolParams};

const DEFAULT_NUM_RESULTS: u32 = 5;
const MAX_NUM_RESULTS: u32 = 20;

/// The SearXNG instance's own base URL, e.g. `http://localhost:8080` for the
/// `searxng/compose.yaml` service reached over this backend's host networking (see
/// `compose.yaml`). Read fresh per call rather than cached at startup — this tool is a
/// zero-arg unit struct, like every other tool, so there's nowhere to stash a
/// constructor-time value.
fn searxng_base_url() -> String {
    std::env::var("SEARXNG_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}

pub struct SearchQueryTool;

#[derive(Deserialize, tool_derive::ToolParams)]
struct SearchQueryArgs {
    #[tool(description = "The search query, e.g. 'rust async trait object safety'.")]
    query: String,
    #[tool(description = "Maximum number of results to return. Defaults to 5, capped at 20.")]
    num_results: Option<u32>,
}

#[derive(Serialize)]
struct SearchResultOut {
    title: String,
    url: String,
    snippet: String,
}

#[derive(Serialize)]
struct SearchQueryOut {
    query: String,
    results: Vec<SearchResultOut>,
}

#[derive(Deserialize)]
struct SearxngResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
}

#[derive(Deserialize)]
struct SearxngResponse {
    #[serde(default)]
    results: Vec<SearxngResult>,
}

#[async_trait]
impl Tool for SearchQueryTool {
    fn function_name(&self) -> &str {
        "web.search_query"
    }

    fn description(&self) -> &str {
        "Runs a web search (via a local SearXNG meta-search instance aggregating multiple search \
         engines) and returns matching results as title/url/snippet triples. Use this whenever \
         you don't already know something and it's the kind of thing that changed or was decided \
         after your training data, or is otherwise not something you'd know from training alone \
         — current events, a specific person/product/library you're unsure about, recent \
         documentation, etc. Follow up with web.request or web.download_file on a result's URL \
         to read more than the snippet."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        SearchQueryArgs::tool_properties()
    }

    // A search query has no side effects and touches nothing but this local search
    // instance — same standing-Allowed default as a plain lookup tool (see
    // `Tool::is_dangerous`'s own doc comment).

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: SearchQueryArgs = serde_json::from_value(data)?;
        let num_results = args.num_results.unwrap_or(DEFAULT_NUM_RESULTS).min(MAX_NUM_RESULTS);
        let base_url = searxng_base_url();

        let response = reqwest::Client::new()
            .get(format!("{base_url}/search"))
            .query(&[("q", args.query.as_str()), ("format", "json")])
            .send()
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't reach searxng at {base_url}: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            return Err(ToolError::FailedUnknown(format!("searxng returned HTTP {status}")));
        }

        let parsed: SearxngResponse = response
            .json()
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't parse searxng's response: {e}")))?;

        let results = parsed
            .results
            .into_iter()
            .take(num_results as usize)
            .map(|r| SearchResultOut { title: r.title, url: r.url, snippet: r.content })
            .collect();

        Ok(serde_json::to_value(SearchQueryOut { query: args.query, results })?)
    }
}
