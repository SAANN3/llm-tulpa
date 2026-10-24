use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use tulpa_plugin::{Block, CallContext, Failure, Host, HttpRequest, HttpResponse, Level, Param, Plugin, Row, Section, Tab, ToolSpec, Value, async_trait, json};

const DEFAULT_GREETING: &str = "Hello";

#[derive(Default)]
struct Hello {
    greeting: Mutex<String>,
    /// The users who chose greetings in capitals. Kept in memory: a restart forgets it, which is fine for an example; a plugin
    /// that must remember writes to its data folder (`Host::data_dir`).
    loud: Mutex<HashSet<i64>>,
    host: OnceLock<Host>,
}

impl Hello {
    fn greeting(&self) -> String {
        self.greeting.lock().unwrap().clone()
    }

    fn is_loud(&self, user_id: i64) -> bool {
        self.loud.lock().unwrap().contains(&user_id)
    }

    fn tab(&self, user_id: i64) -> Tab {
        Tab::new().section(
            Section::new("Greetings")
                .block(Block::text(format!("The greeting is \"{}\".", self.greeting())))
                .block(Block::switch("loud", "Greet in capitals", self.is_loud(user_id)).help("Applies to the greetings the model makes for you."))
                .block(Block::items("Nobody to greet.", vec![Row::new("world", "The world").subtitle("Tries the greeting out").button("greet", "Say hello")])),
        )
    }
}

#[async_trait]
impl Plugin for Hello {
    async fn configure(&self, settings: Value, host: Host) -> Result<(), Failure> {
        let greeting = settings["greeting"].as_str().map(str::trim).filter(|g| !g.is_empty()).unwrap_or(DEFAULT_GREETING);
        *self.greeting.lock().unwrap() = greeting.to_string();
        let _ = self.host.set(host);
        Ok(())
    }

    fn tools(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec::new("hello.greet", "Greets a person by name. Use it when the user asks for a greeting from the hello plugin.")
                .param(Param::string("name", "Who to greet").optional()),
        ]
    }

    async fn call_tool(&self, tool: &str, args: Value, ctx: CallContext) -> Result<Value, Failure> {
        match tool {
            "hello.greet" => {
                let name = args["name"].as_str().filter(|n| !n.trim().is_empty()).unwrap_or("there");
                ctx.host.log(Level::Info, &format!("greeting {name} for user {}", ctx.user_id)).await;
                let mut greeting = format!("{}, {name}! (from the external hello plugin, for user {})", self.greeting(), ctx.user_id);
                if self.is_loud(ctx.user_id) {
                    greeting = greeting.to_uppercase();
                }
                Ok(json!({"greeting": greeting}))
            }
            other => Err(Failure::not_found(format!("no such tool: {other}"))),
        }
    }

    async fn enable(&self) -> Result<(), Failure> {
        if let Some(host) = self.host.get() {
            host.log(Level::Info, "switched on").await;
        }
        Ok(())
    }

    // The same text every turn while nothing it describes changes, so the model server keeps its cache of the chat
    async fn prompt_section(&self, _user_id: i64, tools: bool) -> Option<String> {
        tools.then(|| "The hello plugin is on: when the user asks to be greeted, call hello.greet.".to_string())
    }

    async fn tab_layout(&self, user_id: i64) -> Result<Tab, Failure> {
        Ok(self.tab(user_id))
    }

    async fn tab_action(&self, user_id: i64, action: &str, args: Value) -> Result<Tab, Failure> {
        match action {
            "loud" => {
                let on = args["on"].as_bool().ok_or_else(|| Failure::bad_args("the switch sent no value"))?;
                let mut loud = self.loud.lock().unwrap();
                if on {
                    loud.insert(user_id);
                } else {
                    loud.remove(&user_id);
                }
                drop(loud);
                Ok(self.tab(user_id).message(if on { "Greetings are in capitals now." } else { "Greetings are as they are." }))
            }
            "greet" => Ok(self.tab(user_id).message(format!("{}, world!", self.greeting()))),
            other => Err(Failure::not_found(format!("no such action: {other}"))),
        }
    }

    async fn http_request(&self, request: HttpRequest) -> HttpResponse {
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/greeting") => HttpResponse::json(200, &json!({"greeting": self.greeting(), "asked_by": request.user_id})),
            _ => HttpResponse::status(404),
        }
    }

    async fn user_deleted(&self, user_id: i64) {
        self.loud.lock().unwrap().remove(&user_id);
    }
}

fn main() -> Result<(), tulpa_plugin::Error> {
    tulpa_plugin::serve(Hello::default())
}
